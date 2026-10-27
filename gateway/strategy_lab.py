"""Offline institutional research, never connected to live order admission.

Accepts only explicit MT5 DEAL evidence, never Paper or inferred trades.
Four-mode results use closed-deal net economic PnL; no success is guaranteed.
"""
from __future__ import annotations
import math
import random
from statistics import mean

MODES=('sniper','scalping','swing','expert')
FIELDS=('deal_ticket','closed_ms','mode','symbol','login','server','target',
        'profit','commission','swap','fee')


def load_verified_deals(value):
    if not isinstance(value,list) or len(value)>500_000:
        raise ValueError('Expected a bounded list of MT5 broker close deals')
    output=[]
    seen=set()
    account=None
    for record in value:
        if not isinstance(record,dict) or any(k not in record for k in FIELDS):
            raise ValueError('Missing verified broker close-deal fields')
        if record['target']!='demo':
            raise ValueError('Only independently verified MT5 Demo trades are supported')
        mode=record['mode']
        if mode not in MODES or not isinstance(record['symbol'],str) or not record['symbol']:
            raise ValueError('Unknown strategy mode or symbol')
        ticket=record['deal_ticket'];closed=record['closed_ms']
        if type(ticket) is not int or ticket<=0 or type(closed) is not int or closed<=0:
            raise ValueError('Invalid broker ticket or UTC close time')
        key=(record['login'],record['server'],record['target'])
        if not isinstance(key[0],int) or key[0]<=0 or not isinstance(key[1],str) or not key[1]:
            raise ValueError('Broker account identity unavailable')
        if account is None:account=key
        if key!=account:raise ValueError('Mixing broker accounts would corrupt the evaluation')
        if ticket in seen:raise ValueError('Duplicate broker deal ticket')
        seen.add(ticket)
        prices=[]
        for field in ('profit','commission','swap','fee'):
            n=record[field]
            if isinstance(n,bool) or not isinstance(n,(int,float)) or not math.isfinite(float(n)):
                raise ValueError(f'Invalid {field}; actual broker costs are required')
            prices.append(float(n))
        net=sum(prices)
        if not math.isfinite(net):raise ValueError('Invalid computed broker net result')
        output.append({'deal_ticket':ticket,'closed_ms':closed,'mode':mode,'symbol':record['symbol'],
                       'net':net,'profit':prices[0]})
    output.sort(key=lambda r:(r['closed_ms'],r['deal_ticket']))
    return output


def _ci_for_average(values,seed=7,samples=500):
    if len(values)<30:return None
    randomizer=random.Random(seed)
    n=len(values)
    draws=sorted(mean([values[randomizer.randrange(n)] for _ in range(n)]) for _ in range(samples))
    return [round(draws[int(.025*samples)],6),round(draws[int(.975*samples)-1],6)]


def mode_stats(trades):
    nets=[t['net'] for t in trades]
    high=0.0; cumulative=0.0; drawdown=0.0
    for net in nets:
        cumulative+=net
        high=max(high,cumulative)
        drawdown=max(drawdown,high-cumulative)
    wins=[v for v in nets if v>0];losses=[v for v in nets if v<0]
    profit_factor=(sum(wins)/-sum(losses)) if losses else None
    return {'deals':len(nets), 'net':round(sum(nets),6),
            'win_rate':round(len(wins)/len(nets),6) if nets else None,
            'average_net_per_deal':round(mean(nets),6) if nets else None,
            'max_realized_deal_drawdown':round(drawdown,6),
            'profit_factor':round(profit_factor,6) if profit_factor is not None else None,
            'average_net_95pct_bootstrap':_ci_for_average(nets),
            'evidence':'broker_demo_closed_deals' if nets else 'no_broker_evidence',
            'sample_sufficient_for_ci':len(nets)>=30}


def study(raw,train_fraction=.7,embargo_hours=24):
    if not isinstance(train_fraction,(float,int)) or not 0.5<=train_fraction<=.85:
        raise ValueError('Training split fraction must be 0.5 to 0.85')
    if type(embargo_hours) is not int or not 1<=embargo_hours<=168:
        raise ValueError('Embargo must be 1..168 hours')
    deals=load_verified_deals(raw)
    boundary=math.floor(len(deals)*train_fraction)
    if boundary and boundary<len(deals):
        # Date-based holdout with embargo on both sides of split to reduce
        # cross-fold temporal contamination. Single identical timestamps don't
        # yield artificial chronological independence.
        split_time=deals[boundary]['closed_ms']
        train=[r for r in deals if r['closed_ms']<split_time-embargo_hours*3600000]
        unseen=[r for r in deals if r['closed_ms']>=split_time+embargo_hours*3600000]
    else:
        train=[];unseen=[]
    per={}
    for mode in MODES:
        per[mode]={'overall':mode_stats([r for r in deals if r['mode']==mode]),
                   'train':mode_stats([r for r in train if r['mode']==mode]),
                   'unseen':mode_stats([r for r in unseen if r['mode']==mode])}
    return {'schema':1,'source':'operator_supplied_MT5_Demo_broker_close_deal_evidence',
            'raw_closed_deals':len(deals),'train_deals':len(train),'unseen_deals':len(unseen),
            'embargo_hours':embargo_hours,'modes':per,
            'automated_promotion_authorized':False,
            'conclusion':'UNVERIFIED' if len(deals)<120 or any(per[m]['unseen']['deals']<30 for m in MODES) else 'REVIEW_REQUIRED',
            'limitations':[
                'An externally labeled strategy/mode is not cryptographically verified by this tool.',
                'Deals are not risk-normalized and unpaired Expert comparisons are not causal.',
                'Bootstrap assumes independent deals and does not account for serial dependence.',
                'This research output never changes live broker execution or model weights.',
                'A positive backtest/sample is not evidence of future returns.'
            ]}
