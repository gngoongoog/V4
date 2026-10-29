"""Optional, independent operational admission controls for *new* MT5 entries.

Existing orders, closes, reconciliation, durable journal and broker identity
checks never depend on the optional calendar. Calendar input is OPERATOR
supplied and is NOT independently verified or an automated news feed.
"""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
import json
import math
import os
import sqlite3
import time


def _utc_milliseconds(iso):
    if not isinstance(iso, str) or not iso.endswith('Z'):
        raise ValueError('Calendar timestamps must be UTC ISO-8601 with Z suffix')
    stamp = datetime.fromisoformat(iso[:-1] + '+00:00')
    if stamp.utcoffset().total_seconds() != 0:
        raise ValueError('Non-UTC calendar timestamp')
    return int(stamp.timestamp() * 1000)


def calendar_entry_blocked(policy_path: Path, symbol: str, now_ms: int) -> str | None:
    """Fail-closed when explicitly enabled; no calendar file means no opt-in.

    Avoid accidentally granting an entry on a corrupted policy. Event timestamps
    are UTC and a manually provided scheduled event is NOT a validated source.
    """
    flag = os.environ.get('BRAINTRADE_EVENT_BLACKOUT', '0')
    if flag not in ('0', '1'):
        return 'Event blackout environment flag invalid; entries suspended'
    if flag == '0':
        return None
    try:
        if not policy_path.is_file() or policy_path.stat().st_size > 64_000:
            raise ValueError('Calendar policy missing/oversized')
        doc = json.loads(policy_path.read_text('utf-8'))
        if not isinstance(doc, dict) or set(doc) != {'schema', 'generated_utc', 'events', 'enabled'}:
            raise ValueError('Invalid calendar policy keys')
        if type(doc['schema']) is not int or doc['schema'] != 1 or doc['enabled'] is not True:
            raise ValueError('Calendar policy is not enabled and verified by operator')
        generated = _utc_milliseconds(doc['generated_utc'])
        if not (-60_000 <= now_ms - generated <= 48 * 3_600_000):
            raise ValueError('Calendar schedule not refreshed within 48 hours')
        events = doc['events']
        if not isinstance(events, list) or len(events) > 256:
            raise ValueError('Invalid calendar events')
        seen = set()
        for event in events:
            if not isinstance(event, dict) or set(event) != {'id', 'scheduled_utc', 'impact', 'symbols', 'pre_minutes', 'post_minutes', 'source'}:
                raise ValueError('Invalid calendar event')
            identity = event['id']
            if not isinstance(identity, str) or not 1 <= len(identity) <= 80 or identity in seen:
                raise ValueError('Invalid or repeated calendar ID')
            seen.add(identity)
            if event['impact'] not in ('high', 'medium'):
                raise ValueError('Unsupported calendar impact')
            if not isinstance(event['source'], str) or not 4 <= len(event['source']) <= 200:
                raise ValueError('Calendar requires a human-auditable source')
            affected = event['symbols']
            if not isinstance(affected, list) or not affected or len(affected) > 50 or any(not isinstance(s, str) or len(s) > 80 for s in affected):
                raise ValueError('Calendar symbols invalid')
            pre, post = event['pre_minutes'], event['post_minutes']
            if any(type(x) is not int or not 0 <= x <= 240 for x in (pre, post)):
                raise ValueError('Calendar event window invalid')
            moment = _utc_milliseconds(event['scheduled_utc'])
            if (symbol in affected or '*' in affected) and event['impact'] == 'high':
                if moment - pre * 60_000 <= now_ms <= moment + post * 60_000:
                    return f'High-impact calendar blackout ({identity}); new entries paused'
        return None
    except (ValueError, KeyError, TypeError, UnicodeError, OSError, OverflowError) as exc:
        return 'Event calendar is unavailable/invalid; new entries paused: ' + type(exc).__name__


def initialise_quality_table(db: sqlite3.Connection) -> None:
    db.execute('''CREATE TABLE IF NOT EXISTS execution_quality (
      id TEXT PRIMARY KEY, account_key TEXT NOT NULL, symbol TEXT NOT NULL,
      kind TEXT NOT NULL, state TEXT NOT NULL, recorded_ms INTEGER NOT NULL,
      send_ms REAL, queue_ms REAL, retcode INTEGER)''')
    db.execute('CREATE INDEX IF NOT EXISTS ix_execution_quality_window ON execution_quality(account_key,symbol,recorded_ms)')
    db.commit()


def quality_account_key(cmd: dict) -> str:
    return json.dumps([cmd['login'],cmd['server'],cmd['currency'],cmd['target']], separators=(',',':'),ensure_ascii=False)


def record_execution_quality(db: sqlite3.Connection, cmd: dict, result: dict, now_ms: int) -> None:
    """Only broker send observations; call strictly AFTER durable result commit.

    Duplicate/reconciled intent IDs never inflate quality event counters.
    """
    code = result.get('retcode')
    if code is None and result.get('send_ms') is None:
        return
    code = int(code) if isinstance(code, int) else None
    def finite_nonnegative(value):
        return float(value) if isinstance(value,(int,float)) and math.isfinite(float(value)) and value >= 0 else None
    db.execute('''INSERT INTO execution_quality
      (id,account_key,symbol,kind,state,recorded_ms,send_ms,queue_ms,retcode)
      VALUES(?,?,?,?,?,?,?,?,?)
      ON CONFLICT(id) DO UPDATE SET state=excluded.state,
        retcode=COALESCE(excluded.retcode,execution_quality.retcode)
      WHERE execution_quality.account_key=excluded.account_key
        AND execution_quality.symbol=excluded.symbol AND execution_quality.kind=excluded.kind
        AND ((execution_quality.state='unknown' AND excluded.state IN ('placed','partial','filled','cancelled','rejected'))
          OR (execution_quality.state IN ('placed','partial') AND excluded.state IN ('filled','cancelled','rejected')))''',(
        cmd['id'], quality_account_key(cmd), cmd['symbol'], cmd['kind'],
        result['state'], int(now_ms), finite_nonnegative(result.get('send_ms')),
        finite_nonnegative(result.get('queue_to_send_ms')), code))
    db.commit()


def quality_entry_blocked(db: sqlite3.Connection, cmd: dict, now_ms: int) -> str | None:
    """Opt-in breaker by actual broker retcodes, not unfilled research signals.

    Real default ON, Demo default OFF; use BRAINTRADE_QUALITY_CIRCUIT=0/1 to override.
    Open entries only, never force close/cancel, and never retry a broker send.
    """
    flag = os.environ.get('BRAINTRADE_QUALITY_CIRCUIT', '1' if cmd.get('target') == 'live' else '0')
    if flag not in ('0','1'):
        return 'Execution quality circuit setting invalid; entries suspended'
    if flag == '0':
        return None
    try:
        since = now_ms - 120_000
        rows = db.execute('''SELECT state,retcode FROM execution_quality
            WHERE account_key=? AND symbol=? AND kind='open' AND recorded_ms>=?
            ORDER BY recorded_ms DESC,id DESC LIMIT 20''',
            (quality_account_key(cmd),cmd['symbol'],since)).fetchall()
        rejects = sum(state == 'rejected' and code is not None for state,code in rows)
        unknown = sum(state == 'unknown' for state,code in rows)
        if unknown >= 1:
            return 'Execution quality: unresolved broker submission; entries paused pending reconciliation'
        if rejects >= 3:
            return 'Execution quality: three or more broker rejections within 120 seconds'
    except (sqlite3.Error, KeyError, TypeError, ValueError):
        return 'Execution quality evidence unavailable; new entries suspended'
    return None


def quality_report(db: sqlite3.Connection, limit: int = 1000) -> dict:
    """Verified broker-send timing sample, never a performance/win-rate claim."""
    if type(limit) is not int or not 1 <= limit <= 10_000:
        raise ValueError('Quality report limit must be 1..10000')
    rows = db.execute('SELECT kind,state,send_ms,queue_ms,retcode FROM execution_quality ORDER BY recorded_ms DESC LIMIT ?', (limit,)).fetchall()
    def summary(index):
        measures = sorted(float(r[index]) for r in rows if isinstance(r[index], (int,float)) and math.isfinite(r[index]) and r[index] >= 0)
        if not measures:
            return {'samples':0,'median_ms':None,'p95_ms':None,'p99_ms':None}
        def pct(x): return measures[max(0, math.ceil((len(measures) * x)) - 1)]
        return {'samples':len(measures),'median_ms':pct(.5),'p95_ms':pct(.95),'p99_ms':pct(.99)}
    return {
        'schema':1, 'observations':len(rows),
        'broker_rejections':sum(r[1] == 'rejected' and r[4] is not None for r in rows),
        'unknown':sum(r[1] == 'unknown' for r in rows),
        'fills':sum(r[1] in ('filled','partial') for r in rows),
        'broker_send':summary(2),'queue_to_send':summary(3),
        'caveat':'Time measured within Python adapter. Not network roundtrip to exchange or a fill guarantee.'
    }
