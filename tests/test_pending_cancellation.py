"""Synthetic broker tests only: no real account or MetaTrader connection."""
import copy
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace as N
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from execution import BrokerExecutor,clock_ms
from test_broker_execution import Broker

class CancellationBroker(Broker):
    TRADE_ACTION_REMOVE=8
    ORDER_STATE_CANCELED=2;ORDER_STATE_FILLED=4;ORDER_STATE_REJECTED=5;ORDER_STATE_EXPIRED=6
    def __init__(self):
        super().__init__()
        self.order=N(ticket=42,symbol='EURUSD',magic=410618,type=2,time_setup_msc=123000,time_expiration=0,
                     volume_initial=.1,volume_current=.1,price_open=1.1,sl=1.0,tp=1.2,price_stoplimit=0.)
        self.active=[self.order];self.history=[];self.apply_change=True
    def orders_get(self,**kwargs):
        ticket=kwargs.get('ticket');return [x for x in self.active if ticket is None or x.ticket==ticket]
    def history_orders_get(self,**kwargs):return [x for x in self.history if x.ticket==kwargs['ticket']]
    def order_send(self,request):
        self.sends.append(copy.deepcopy(request))
        if self.raise_send:raise RuntimeError('connection lost after send')
        if self.apply_change and self.code==10009:
            done=copy.copy(self.order);done.state=self.ORDER_STATE_CANCELED;self.active=[];self.history=[done]
        return N(retcode=self.code,comment='broker')

def command():
    now=clock_ms()
    return dict(id='pending-cancel-1',kind='cancel_pending',state='queued',login=17,server='Demo',currency='USD',
                target='demo',symbol='EURUSD',magic=410618,brain='manual',order=42,expected_order_type=2,
                expected_volume_initial=.1,expected_volume_current=.1,expected_price_open=1.1,expected_sl=1.,
                expected_tp=1.2,expected_stoplimit=0.,expected_setup_ms=123000,expected_expiration=0,
                created_ms=now,expires_ms=now+30000)

class PendingCancellationTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.path=Path(self.tmp.name)/'journal.sqlite'
        self.ex=BrokerExecutor(self.path);self.mt=CancellationBroker();self.cmd=command()
    def tearDown(self):self.ex.close();self.tmp.cleanup()
    def execute(self,authorize=lambda _:True):return self.ex.execute(self.mt,self.cmd,authorize)
    def test_exact_ticket_cancel_is_confirmed_only_by_terminal_history(self):
        result=self.execute();self.assertEqual(result['state'],'cancelled');self.assertTrue(result['terminal_order']['confirmed'])
        self.assertEqual(self.mt.sends,[dict(action=8,order=42)]);self.assertEqual(self.execute()['state'],'cancelled')
        self.assertEqual(len(self.mt.sends),1);self.assertEqual(self.ex.db.execute('SELECT COUNT(*) FROM execution_quality').fetchone()[0],0)
    def test_acknowledgement_without_terminal_history_is_not_cancellation(self):
        self.mt.apply_change=False;self.assertEqual(self.execute()['state'],'placed');self.mt.active=[]
        self.assertEqual(self.execute()['state'],'placed','Disappearance alone proves neither cancellation nor fill')
        done=copy.copy(self.mt.order);done.state=self.mt.ORDER_STATE_FILLED;done.volume_current=0.;self.mt.history=[done]
        result=self.execute();self.assertEqual(result['state'],'filled');self.assertEqual(result['terminal_order']['state'],'filled');self.assertEqual(len(self.mt.sends),1)
    def test_partial_fill_then_cancel_is_distinguished_from_no_fill(self):
        self.mt.order.volume_current=.04;self.cmd['expected_volume_current']=.04;result=self.execute()
        self.assertEqual(result['state'],'cancelled');self.assertEqual(result['terminal_order']['volume_initial'],.1);self.assertEqual(result['terminal_order']['volume_current'],.04)
    def test_interrupted_cancel_never_resends_and_can_reconcile_after_restart(self):
        self.mt.raise_send=True;self.assertEqual(self.execute()['state'],'unknown');self.ex.close();self.ex=BrokerExecutor(self.path)
        self.assertEqual(self.execute()['state'],'unknown');self.assertEqual(len(self.mt.sends),1)
        done=copy.copy(self.mt.order);done.state=self.mt.ORDER_STATE_EXPIRED;self.mt.active=[];self.mt.history=[done]
        self.assertEqual(self.execute()['state'],'expired');self.assertEqual(len(self.mt.sends),1)
    def test_unknown_without_journal_is_reconciled_but_never_sent(self):
        self.cmd['state']='unknown';self.assertEqual(self.execute()['state'],'unknown')
        done=copy.copy(self.mt.order);done.state=self.mt.ORDER_STATE_FILLED;done.volume_current=0.;self.mt.active=[];self.mt.history=[done]
        self.assertEqual(self.execute()['state'],'filled');self.assertEqual(self.mt.sends,[])
    def test_account_owner_and_exact_order_are_checked(self):
        for field,value in [('ticket',43),('magic',410600),('symbol','XAUUSD'),('type',3),('volume_initial',.2),('volume_current',.05),('volume_current',.099999999),('price_open',1.11),('price_open',1.100000001),('sl',.9),('tp',1.3),('price_stoplimit',.5),('time_setup_msc',123001),('time_expiration',1)]:
            original=getattr(self.mt.order,field);setattr(self.mt.order,field,value);self.assertEqual(self.execute()['state'],'rejected',field);setattr(self.mt.order,field,original)
        self.mt.account.trade_mode=2;self.assertEqual(self.execute()['state'],'rejected');self.assertEqual(self.mt.sends,[])
    def test_changed_order_after_journal_is_not_sent(self):
        calls=[]
        def authorize(_):
            calls.append(1)
            if len(calls)==2:self.mt.order.volume_current=.05
            return True
        self.assertEqual(self.execute(authorize)['state'],'cancelled');self.assertEqual(self.mt.sends,[])
        self.mt.order.volume_current=.1;self.assertEqual(self.execute()['state'],'cancelled');self.assertEqual(self.mt.sends,[])
    def test_unresolved_ticket_blocks_a_distinct_cancellation_intent(self):
        self.mt.raise_send=True;self.assertEqual(self.execute()['state'],'unknown');self.cmd['id']='pending-cancel-2'
        self.assertEqual(self.execute()['state'],'unknown');self.assertEqual(len(self.mt.sends),1)

if __name__=='__main__':unittest.main()
