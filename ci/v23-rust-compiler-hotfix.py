"""Apply compiler-derived V23 Rust test-fixture fixes before native Windows compilation.
All replacements are exact and fail closed if the expected source changed.
"""
from pathlib import Path
p=Path("crates/runtime/src/enterprise_supervisor.rs")
s=p.read_text(encoding="utf-8")
literal="as f64*.25"
if s.count(literal)!=4:raise SystemExit("V23 expected 4 Rust leading-dot float literals")
s=s.replace(literal,"as f64*0.25")
old="""    let m=t.market.as_mut().unwrap();
    m.m1[35].time=m.m1[34].time;
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    m.m1[35].time=m.m1[34].time+60;
    m.m1[35].high=m.m1[35].low-1.0;
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);"""
new="""    {
        let m=t.market.as_mut().unwrap();
        m.m1[35].time=m.m1[34].time;
    }
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    {
        let m=t.market.as_mut().unwrap();
        m.m1[35].time=m.m1[34].time+60;
        m.m1[35].high=m.m1[35].low-1.0;
    }
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);"""
if s.count(old)!=1:raise SystemExit("V23 expected Rust E0502 borrow fixture not found exactly once")
s=s.replace(old,new)
p.write_text(s,encoding="utf-8")
print("Rust V23 test fixtures: 4 numeric literal corrections, 1 borrow scope correction")
