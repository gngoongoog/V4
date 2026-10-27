"""Mock MT5 integration tests; does NOT claim live broker acceptance."""
from __future__ import annotations
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
from datetime import datetime, timezone, timedelta
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1] / 'gateway'))
from institutional_controls import (
    calendar_entry_blocked, quality_entry_blocked, quality_report, record_execution_quality,
)
from execution import BrokerExecutor, clock_ms
from test_broker_execution import Broker, command


def stamp(ms):
    return datetime.fromtimestamp(ms/1000,timezone.utc).isoformat(timespec='seconds').replace('+00:00','Z')


def policy(now, *, event_offset=0, severity='high'):
    return dict(schema=1, enabled=True, generated_utc=stamp(now-1000),events=[dict(
        id='CPI-operator-verification-needed',scheduled_utc=stamp(now+event_offset),
        source='Operator supplied event schedule; not automatically verified',
        impact=severity,symbols=['XAUUSD'],pre_minutes=30,post_minutes=30)])


class CalendarPolicyTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.path=Path(self.tmp.name)/'event_calendar.json'
        self.now=clock_ms()
    def tearDown(self):self.tmp.cleanup()
    def write(self,data):self.path.write_text(json.dumps(data),'utf8')
    def evaluate(self,now=None,flag='1',sym='XAUUSD'):
        with patch.dict(os.environ,{'BRAINTRADE_EVENT_BLACKOUT':flag}):
            return calendar_entry_blocked(self.path,sym,self.now if now is None else now)
    def test_default_off_is_explicit_and_does_not_hide_missing_source(self):
        self.assertIsNone(self.evaluate(flag='0'))
        self.assertIn('paused',self.evaluate())
    def test_high_impact_blocks_during_window_and_releases_after(self):
        self.write(policy(self.now))
        self.assertIn('CPI-',self.evaluate())
        self.assertIsNone(self.evaluate(now=self.now+31*60_000))
    def test_other_symbol_and_medium_impact_not_blocked(self):
        self.write(policy(self.now))
        self.assertIsNone(self.evaluate(sym='EURUSD'))
        self.write(policy(self.now,severity='medium'))
        self.assertIsNone(self.evaluate())
    def test_stale_schedule_fail_closed(self):
        data=policy(self.now);data['generated_utc']=stamp(self.now-49*3600_000)
        self.write(data)
        self.assertIn('paused',self.evaluate())
    def test_malformed_and_future_timezone_fail_closed(self):
        self.path.write_text('{broken','utf8')
        self.assertIn('paused',self.evaluate())
        data=policy(self.now);data['events'][0]['scheduled_utc']='2026-10-23T12:00:00+03:00'
        self.write(data)
        self.assertIn('paused',self.evaluate())
    def test_duplicate_id_bad_window_and_source_are_rejected(self):
        data=policy(self.now);data['events'].append(data['events'][0].copy())
        self.write(data);self.assertIn('paused',self.evaluate())
        data=policy(self.now);data['events'][0]['pre_minutes']=-5
        self.write(data);self.assertIn('paused',self.evaluate())
        data=policy(self.now);data['events'][0]['source']=''
        self.write(data);self.assertIn('paused',self.evaluate())


class InstitutionalOrderTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.dbpath=Path(self.tmp.name)/'broker-journal.sqlite'
        self.executor=BrokerExecutor(self.dbpath)
        self.mt5=Broker()
        self.cmd=command()
    def tearDown(self):self.executor.close();self.tmp.cleanup()
    def send(self):return self.executor.execute(self.mt5,self.cmd,lambda intent:True)
    def test_open_blocked_before_send_on_event_and_owned_close_exempt(self):
        (self.dbpath.parent/'event_calendar.json').write_text(json.dumps(policy(clock_ms())),'utf8')
        with patch.dict(os.environ,{'BRAINTRADE_EVENT_BLACKOUT':'1'}):
            result=self.send()
            self.assertEqual(result['state'],'rejected')
            self.assertIn('blackout',result['reason'])
            self.assertEqual(self.mt5.sends,[])
            # Closing existing owned ticket ignores calendar, not ownership checks.
            self.mt5.positions=[__import__('types').SimpleNamespace(ticket=41,magic=self.cmd['magic'],symbol='XAUUSD',volume=.1,type=self.mt5.POSITION_TYPE_BUY)]
            self.cmd.update(id='closing-owned-ticket',kind='close',position=41)
            self.assertEqual(self.send()['state'],'filled')
            self.assertEqual(len(self.mt5.sends),1)
    def test_calendar_changes_after_journal_cancel_without_broker_send(self):
        called=0
        def change(cmd):
            nonlocal called
            called+=1
            if called==2:raise ValueError('High-impact event arrived after journal')
        with patch.object(self.executor,'_institutional_entry_guard',side_effect=change):
            result=self.send()
        self.assertEqual(result['state'],'cancelled')
        self.assertEqual(called,2)
        self.assertFalse(self.mt5.sends)
        saved=self.executor.db.execute('SELECT result FROM sends WHERE id=?',(self.cmd['id'],)).fetchone()
        self.assertEqual(json.loads(saved[0])['state'],'cancelled')
    def test_broker_rejection_circuit_requires_three_distinct_sends(self):
        self.mt5.code=10006
        with patch.dict(os.environ,{'BRAINTRADE_QUALITY_CIRCUIT':'1'}):
            for i in range(3):
                self.cmd['id']=f'rejection-{i}'
                self.assertEqual(self.send()['state'],'rejected')
            self.cmd['id']='fourth-attempt'
            result=self.send()
            self.assertIn('three or more',result['reason'])
            self.assertEqual(len(self.mt5.sends),3)
            self.assertEqual(quality_report(self.executor.db)['broker_rejections'],3)
    def test_unknown_submission_blocks_followups_without_retry(self):
        self.mt5.raise_send=True
        with patch.dict(os.environ,{'BRAINTRADE_QUALITY_CIRCUIT':'1'}):
            self.assertEqual(self.send()['state'],'unknown')
            self.assertEqual(len(self.mt5.sends),1)
            self.cmd['id']='other-open-after-unknown'
            second=self.send()
            self.assertEqual(second['state'],'rejected')
            self.assertIn('unresolved',second['reason'])
            self.assertEqual(len(self.mt5.sends),1)
    def test_duplicate_command_does_not_double_count_broker_observations(self):
        self.assertEqual(self.send()['state'],'filled')
        self.assertEqual(self.send()['state'],'filled')
        self.assertEqual(quality_report(self.executor.db)['observations'],1)
    def test_quality_report_percentiles_are_accurate_and_omit_missing(self):
        for i in range(20):
            cmd=dict(self.cmd,id=f'observation-{i}')
            result=dict(state='filled',send_ms=float(i+1),queue_to_send_ms=2*i,retcode=10009)
            record_execution_quality(self.executor.db,cmd,result,clock_ms())
        report=quality_report(self.executor.db)
        self.assertEqual(report['broker_send']['samples'],20)
        self.assertEqual(report['broker_send']['median_ms'],10.)
        self.assertEqual(report['broker_send']['p95_ms'],19.)
        self.assertEqual(report['broker_send']['p99_ms'],20.)
        self.assertEqual(report['queue_to_send']['p95_ms'],36.)
    def test_exit_not_blocked_by_quality_circuit(self):
        self.mt5.positions=[__import__('types').SimpleNamespace(ticket=41,magic=self.cmd['magic'],symbol='XAUUSD',volume=.1,type=self.mt5.POSITION_TYPE_BUY)]
        self.cmd.update(kind='close',position=41)
        with patch.dict(os.environ,{'BRAINTRADE_QUALITY_CIRCUIT':'1','BRAINTRADE_EVENT_BLACKOUT':'1'}):
            self.assertEqual(self.send()['state'],'filled')
            self.assertEqual(len(self.mt5.sends),1)

if __name__=='__main__':unittest.main()
