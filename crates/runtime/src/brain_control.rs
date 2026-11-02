//! Account/symbol/mode scoped advisor controls and explicitly sampled activity.
use anyhow::Result;
use braintrade_domain::{execution::{Signal, BRAINS}, trading::TradingMode};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control { pub advisor: bool, pub paper: bool }
impl Default for Control {fn default()->Self {Self{advisor:true,paper:true}}}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS brain_controls(context TEXT NOT NULL,mode TEXT NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,mode,brain));
        CREATE TABLE IF NOT EXISTS brain_activity(context TEXT NOT NULL,mode TEXT NOT NULL,brain TEXT NOT NULL,checks INTEGER NOT NULL,signals INTEGER NOT NULL,last_key TEXT NOT NULL,last_ms INTEGER NOT NULL,PRIMARY KEY(context,mode,brain));")?;
    Ok(())
}
fn mode_key(mode:TradingMode)->String {serde_json::to_string(&mode).unwrap()}
pub fn load(db:&Connection,key:&str,mode:TradingMode)->Result<BTreeMap<String,Control>> {
    let mut stmt=db.prepare("SELECT brain,payload FROM brain_controls WHERE context=?1 AND mode=?2")?;
    let rows=stmt.query_map(params![key,mode_key(mode)],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id,p)|Ok((id,serde_json::from_str(&p)?))).collect()
}
pub fn account_key(key:&str,account_mode:Option<i32>)->String {
    serde_json::to_string(&(key,"broker-mode-v1",account_mode)).unwrap()
}
/// Legacy controls have no broker-mode field. Seed a missing new control
/// conservatively from the legacy value, but all subsequent writes are scoped
/// to Demo/REAL independently; the old rows are never removed.
pub fn load_account(db:&Connection,key:&str,mode:TradingMode,account_mode:Option<i32>)->Result<BTreeMap<String,Control>> {
    let mut scoped=load(db,&account_key(key,account_mode),mode)?;
    for (id,value) in load(db,key,mode)? {scoped.entry(id).or_insert(value);}
    Ok(scoped)
}
pub fn save(db:&Connection,key:&str,mode:TradingMode,id:&str,c:Control)->Result<()> {
    anyhow::ensure!(id!="system" && BRAINS.iter().any(|(b,_)|*b==id),"اختر عقلاً فرعياً معروفاً");
    anyhow::ensure!(!key.is_empty(),"اربط حساب MT5 أولاً");
    db.execute("INSERT OR REPLACE INTO brain_controls VALUES(?1,?2,?3,?4)",params![key,mode_key(mode),id,serde_json::to_string(&c)?])?;
    Ok(())
}
/// Change Paper admission for every secondary brain in one transaction.
/// Existing advisor choices are preserved, including inherited legacy values.
/// This does not stop analytical learning or cancel already-open Paper trades.
pub fn set_all_paper(db:&Connection,key:&str,mode:TradingMode,account_mode:Option<i32>,paper:bool)->Result<BTreeMap<String,Control>> {
    anyhow::ensure!(!key.is_empty(),"اربط حساب MT5 أولاً");
    anyhow::ensure!(matches!(account_mode,Some(0|2)),"نوع حساب MT5 غير مؤكد");
    let tx=db.unchecked_transaction()?;
    let mut controls=load_account(&tx,key,mode,account_mode)?;
    let scoped=account_key(key,account_mode);
    for (id,_) in BRAINS.iter().filter(|(id,_)|*id!="system") {
        let mut control=controls.get(*id).copied().unwrap_or_default();
        control.paper=paper;
        save(&tx,&scoped,mode,id,control)?;
        controls.insert((*id).to_owned(),control);
    }
    tx.commit()?;
    Ok(controls)
}
/// Called at most once per second, outside the per-tick accounting path. Counts
/// ready observations with a new key/direction, NOT fills or every broker tick.
pub fn sample(db:&Connection,key:&str,mode:TradingMode,id:&str,s:&Signal,now:u64)->Result<()> {
    let mark=if s.ready && !s.key.is_empty() {format!("{}:{}",s.key,s.side)}else{String::new()};
    db.execute("INSERT INTO brain_activity VALUES(?1,?2,?3,1,CASE WHEN ?4='' THEN 0 ELSE 1 END,?4,?5)
        ON CONFLICT(context,mode,brain) DO UPDATE SET checks=checks+1,
        signals=signals+CASE WHEN excluded.last_key<>'' AND excluded.last_key<>last_key THEN 1 ELSE 0 END,
        last_key=CASE WHEN excluded.last_key='' THEN last_key ELSE excluded.last_key END,last_ms=excluded.last_ms",
        params![key,mode_key(mode),id,mark,now])?;
    Ok(())
}
/// What the CURRENT coordinator really does with this brain. These fields
/// describe *decision eligibility*, not whether a market signal exists, was
/// filled, or produced valid out-of-sample learning evidence.
fn participation(
    mode:TradingMode,id:&str,owner:Option<&str>,
    control:Control,return_pending:bool,sniper_selected:Option<bool>,
)->(Option<bool>,&'static str,&'static str) {
    if owner.is_none() {
        return (None,"unverified","حساب التنفيذ أو الوضع غير مؤكد؛ لا ننسب للعقل مشاركة فعلية");
    }
    if id=="system" {
        return (Some(false),"system_owner","هذا منسق التنفيذ وليس مستشاراً فرعياً");
    }
    if owner==Some(id) {
        return (Some(false),"solo_owner","هذا العقل هو المنفّذ المنفرد؛ زر المشاركة لا يتحكم بصلاحية SOLO");
    }
    if owner!=Some("system") {
        return (Some(false),"solo_other","عقل آخر يملك تنفيذ SOLO؛ بقية المستشارين للمراقبة وPaper");
    }
    if return_pending {
        return (Some(false),"return_pending","أوامر أو مركز قديم لهذا العقل لم تُسوَّ بعد؛ لا ينضم للمنسق");
    }
    // A disabled advisor toggle cannot make Classic Sniper suddenly depend
    // on Learner. Check the selected Sniper engine before the toggle.
    if mode==TradingMode::Sniper && id=="sniper_learner" {
        match sniper_selected {
            Some(false)=>return (Some(false),"sniper_classic_selected",
                "القناص الكلاسيكي مختار؛ غيّر إعداد العقل إلى Learner قبل تعديل المشاركة"),
            None=>return (None,"sniper_selection_unverified",
                "اختيار القناص Classic/Learner غير مؤكد من Core؛ لا ننسب مشاركة"),
            Some(true)=>{}
        }
    }
    if !control.advisor {
        return (Some(false),"advisor_disabled","المشاركة متوقفة لهذا العقل في الحساب والوضع الحاليين");
    }
    match mode {
        TradingMode::Expert =>
            (Some(true),"expert_eligible","مؤهل للتصويت في Expert إذا أعطى إشارة جاهزة؛ هذا ليس تنفيذ صفقة أو إثبات ربح"),
        TradingMode::Sniper if id=="sniper_learner" =>
            (Some(true),"sniper_learner_eligible","القناص المتعلّم مختار ومشارك؛ تبقى بوابات الإشارة والمخاطر والتنفيذ مستقلة"),
        TradingMode::Sniper =>
            (Some(false),"sniper_diagnostic","هذا العقل تشخيصي في وضع Sniper، وليس مصدراً لتغيير قرار القناص"),
        TradingMode::Scalping =>
            (Some(false),"scalping_diagnostic","العقول الفرعية تشخيصية في Scalping؛ إشارة الوضع الرئيسي لا تُحجب بتصويتها"),
        TradingMode::Swing =>
            (Some(false),"swing_diagnostic","العقول الفرعية تشخيصية في Swing؛ إشارة الوضع الرئيسي لا تُحجب بتصويتها"),
    }
}

/// Descriptive input provenance only; NEVER a claim that a member is
/// profitable, trained or connected to exchange-wide order flow.
fn data_provenance(id:&str)->(&'static str,&'static str) {
    match id {
        "dom"=>("mt5_broker_market_book",
            "عمق أوامر MT5 المقدم من الوسيط فقط؛ لا يثبت سيولة COMEX أو الصفقات المنفذة"),
        "profile"=>("candle_tick_volume_proxy",
            "توزيع تقديري لـ tick_volume الشموع على نطاق OHLC؛ ليس حجم تداول فعلياً عند كل سعر"),
        "gann_dom_profile"=>("mixed_gann_broker_book_candle_proxy",
            "قواعد جان + دفتر أوامر الوسيط + تقدير نشاط الشموع؛ ليس Order Flow مركزياً"),
        "wyckoff"=>("completed_candles_and_tick_volume",
            "قواعد أحداث وايكوف اعتماداً على الشموع ونشاط تيكات الوسيط؛ ليست قراءة مؤسسات مباشرة"),
        "sniper_learner"=>("mt5_quote_ticks_and_matched_demo_deals",
            "إشارات من أسعار MT5 اللحظية؛ تحديث الأوزان يتطلب صفقات Demo مغلقة ومطابقة"),
        "generated"=>("operator_selected_saved_strategy",
            "قاعدة محفوظة اختارها المشغل من مختبر الاستراتيجيات؛ لا تعني اختراعاً تلقائياً مربحاً"),
        "system"=>("selected_mode_coordinator",
            "منسق الوضع المختار؛ يحق له التنفيذ فقط عند تحقق START وبوابات Core"),
        _=>("deterministic_completed_candle_rules",
            "قواعد مؤشرات وأسعار شموع مكتملة؛ ليست نموذج تعلم عميق موثقاً بذاتها"),
    }
}

pub fn enrich(value:&mut Value,db:&Connection,key:&str,mode:TradingMode,account_mode:Option<i32>,sniper_selected:Option<bool>)->Result<()> {
    let controls=load_account(db,key,mode,account_mode)?;
    let selected_owner=value["selected_owner"].as_str().map(str::to_owned);
    let activity_key=account_key(key,account_mode);
    value["trading_mode"]=json!(mode);
    value["activity_note"]=json!("الإشارات: رصد جاهزية بعينة واحدة/ثانية مع حذف تكرار مفتاح الإشارة. ليست عدداً لكل نبضة سعر ولا أوامر منفذة. الإحصاءات أدناه للحساب والرمز والوضع الحالي فقط.");
    let mut decision_eligible=0u64;
    let mut paper_enabled=0u64;
    let mut visible_secondary=0u64;
    for b in value["brains"].as_array_mut().into_iter().flatten() {
        let id=b["config"]["id"].as_str().unwrap_or("").to_owned();
        let control=controls.get(&id).copied().unwrap_or_default();
        b["control"]=json!(control);
        let (source,source_note)=data_provenance(&id);
        b["data_evidence"]=json!({"source":source,"note":source_note,
            "exchange_wide":false,"validated_trading_edge":false});
        let (eligible,status,reason)=participation(mode,&id,selected_owner.as_deref(),
            control,b["return_pending"]==true,sniper_selected);
        b["participates_in_coordinator"]=json!(eligible);
        b["decision_participation"]=json!({"status":status,"reason":reason,
            "eligible_for_mode_decision":eligible,
            "note":"أهلية المشاركة ليست إشارة جاهزة أو صفقة وسيط أو تدريباً مثبتاً"});
        if id!="system" {
            visible_secondary+=1;
            if eligible==Some(true) {decision_eligible+=1;}
            if control.paper {paper_enabled+=1;}
        }
        let activity:Option<(u64,u64,u64)>=db.query_row("SELECT checks,signals,last_ms FROM brain_activity WHERE context=?1 AND mode=?2 AND brain=?3",params![activity_key,mode_key(mode),id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (checks,signals,last)=activity.unwrap_or_default();
        b["activity"]=json!({"checks":checks,"signals":signals,"last_ms":last});
        b["frozen_trial"]=crate::frozen_shadow::report(db,key,mode,&id)?;
        if id=="system"{
            b["frozen_coordinator"]=if mode==TradingMode::Sniper{
                json!({"status":"tick_native_research_not_implemented",
                    "mode":"sniper","no_broker_orders":true,
                    "reason":"M1-close research cannot certify Sniper tick decisions"})
            }else{crate::frozen_coordinator::report(db,key,mode)?};
            if mode==TradingMode::Sniper {
                b["sniper_execution_latency"]=crate::sniper_latency_audit::audit(db,key)?;
                b["sniper_deal_link"]=crate::sniper_deal_link_audit::audit(db,key)?;
                b["sniper_policy_quality"]=crate::sniper_policy_quality::report(db,key)?;
                b["sniper_opportunities"]=crate::sniper_opportunities::report(db,key)?;
                b["sniper_multigate_breakdown"]=crate::sniper_blocker_review::report(db,key,crate::now_ms())?;
                b["sniper_demo_lineage"]=crate::sniper_demo_lineage::report(db,key)?;
            }
        }
        let (closed,wins,net):(u64,u64,f64)=db.query_row("SELECT COUNT(*),COALESCE(SUM(CASE WHEN json_extract(payload,'$.net_paper')>0 THEN 1 ELSE 0 END),0),COALESCE(SUM(json_extract(payload,'$.net_paper')),0.0) FROM paper_brain_outcomes WHERE context=?1 AND brain=?2 AND json_extract(payload,'$.trading_mode_at_open')=?3",params![key,id,serde_json::to_value(mode)?.as_str().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let open:u64=db.query_row("SELECT COUNT(*) FROM paper_brain_positions WHERE context=?1 AND brain=?2",params![key,id],|r|r.get(0))?;
        b["paper_stats"]=json!({"closed":closed,"wins":wins,"net":net,"open_all_modes":open});
        b["quality"]=crate::learning_quality::audit(db,key,mode,&id)?;
        let (orders,fills):(u64,u64)=db.query_row("SELECT COUNT(*),COALESCE(SUM(CASE WHEN CAST(json_extract(result,'$.deal') AS INTEGER)>0 THEN 1 ELSE 0 END),0) FROM broker_commands WHERE context=?1 AND kind='open' AND json_extract(payload,'$.trading_mode')=?2 AND COALESCE(json_extract(payload,'$.signal.coordination.selection.brain'),brain)=?3",params![key,serde_json::to_value(mode)?.as_str().unwrap(),id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        b["attributed_orders"]=json!({"requested":orders,"fill_acknowledged":fills});
    }
    let issue=if selected_owner.is_none(){
        "هوية المنفّذ غير موثقة؛ لا نعرض جاهزية عقول"
    }else if selected_owner.as_deref()!=Some("system"){
        "وضع SOLO: العقل المنفرد فقط يملك فتح الصفقات؛ مشاركة المستشارين لا تتحكم به"
    }else if mode==TradingMode::Expert && decision_eligible==0 {
        "لا يوجد مستشار مؤهل لتصويت Expert؛ لن يكون هناك قرار دخول من إجماع العقول"
    }else if mode==TradingMode::Sniper && sniper_selected==Some(true) && decision_eligible==0 {
        "القناص المتعلّم مختار لكن مشاركته غير مؤهلة؛ لن يفتح الصفقات عبر Learner"
    }else if matches!(mode,TradingMode::Scalping|TradingMode::Swing){
        "Scalping/Swing يعملان بإشارة الوضع الرئيسي؛ المستشارون تشخيصيون ولا يغيّرون الإشارة"
    }else if mode==TradingMode::Sniper && sniper_selected==Some(false){
        "القناص الكلاسيكي هو مصدر إشارة Sniper؛ العقول الفرعية تشخيصية"
    }else{
        "أهلية المشاركة مضبوطة؛ هذا ليس تأكيداً لجاهزية إشارة أو تنفيذ MT5"
    };
    value["mode_participation_summary"]=json!({
        "mode":mode,"selected_owner":selected_owner,
        "secondary_brains":visible_secondary,"eligible_for_decision":decision_eligible,
        "paper_enabled":paper_enabled,
        "source":"current_broker_account_mode_and_core_controls",
        "no_pnl_claim":true,"note":issue
    });
    Ok(())
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn brain_source_labels_never_pretend_to_be_exchange_wide_order_flow() {
        assert_eq!(data_provenance("dom").0,"mt5_broker_market_book");
        assert!(data_provenance("dom").1.contains("COMEX"));
        assert_eq!(data_provenance("profile").0,"candle_tick_volume_proxy");
        assert!(data_provenance("profile").1.contains("OHLC"));
        assert_eq!(data_provenance("gann_dom_profile").0,
            "mixed_gann_broker_book_candle_proxy");
        assert_eq!(data_provenance("generated").0,"operator_selected_saved_strategy");
        assert_eq!(data_provenance("sniper_learner").0,
            "mt5_quote_ticks_and_matched_demo_deals");
    }
    #[test] fn decision_roles_are_truthful_across_all_four_modes_and_solo() {
        let on=Control{advisor:true,paper:true};
        let off=Control{advisor:false,paper:true};
        for mode in [TradingMode::Scalping,TradingMode::Swing] {
            let (effective,status,_)=participation(mode,"trend",Some("system"),on,false,Some(true));
            assert_eq!(effective,Some(false));
            assert!(status.ends_with("_diagnostic"));
        }
        assert_eq!(participation(TradingMode::Expert,"trend",Some("system"),on,false,None).0,Some(true));
        assert_eq!(participation(TradingMode::Expert,"trend",Some("system"),off,false,None).1,"advisor_disabled");
        assert_eq!(participation(TradingMode::Expert,"trend",Some("system"),on,true,None).1,"return_pending");
        assert_eq!(participation(TradingMode::Expert,"gann",Some("trend"),on,false,None).1,"solo_other");
        assert_eq!(participation(TradingMode::Expert,"trend",Some("trend"),off,false,None).1,"solo_owner");
        assert_eq!(participation(TradingMode::Expert,"trend",None,on,false,None).0,None);
        assert_eq!(participation(TradingMode::Sniper,"sniper_learner",Some("system"),on,false,Some(true)).0,Some(true));
        assert_eq!(participation(TradingMode::Sniper,"sniper_learner",Some("system"),on,false,Some(false)).0,Some(false));
        assert_eq!(participation(TradingMode::Sniper,"sniper_learner",Some("system"),off,false,Some(false)).1,
            "sniper_classic_selected","Classic selection must dominate an unused advisor toggle");
        assert_eq!(participation(TradingMode::Sniper,"sniper_learner",Some("system"),off,false,Some(true)).1,
            "advisor_disabled","Actual Learner selection can enable its advisor gate");
        assert_eq!(participation(TradingMode::Sniper,"sniper_learner",Some("system"),on,false,None).0,None);
        assert_eq!(participation(TradingMode::Sniper,"gann",Some("system"),on,false,Some(true)).0,Some(false));
    }
    #[test] fn demo_and_real_controls_separate_after_legacy_seed() {
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        save(&db,"same",TradingMode::Expert,"trend",Control{advisor:false,paper:true}).unwrap();
        assert!(!load_account(&db,"same",TradingMode::Expert,Some(0)).unwrap()["trend"].advisor);
        save(&db,&account_key("same",Some(0)),TradingMode::Expert,"trend",Control{advisor:true,paper:true}).unwrap();
        assert!(load_account(&db,"same",TradingMode::Expert,Some(0)).unwrap()["trend"].advisor);
        assert!(!load_account(&db,"same",TradingMode::Expert,Some(2)).unwrap()["trend"].advisor);
    }
    #[test] fn controls_are_scoped_and_samples_are_deduplicated_across_restart() {
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        save(&db,"account-a",TradingMode::Sniper,"sniper_learner",Control{advisor:false,paper:true}).unwrap();
        assert!(!load(&db,"account-a",TradingMode::Sniper).unwrap()["sniper_learner"].advisor);
        assert!(load(&db,"account-b",TradingMode::Sniper).unwrap().is_empty());
        assert!(load(&db,"account-a",TradingMode::Expert).unwrap().is_empty());
        let mut s=Signal{brain:"trend".into(),side:1,key:"bar:1".into(),bar_time:1,stop_distance:1.,target_distance:2.,hold_ms:1000,reason:String::new(),ready:true,coordination:None};
        sample(&db,"a",TradingMode::Expert,"trend",&s,1000).unwrap();
        init(&db).unwrap();sample(&db,"a",TradingMode::Expert,"trend",&s,2000).unwrap();
        s.ready=false;sample(&db,"a",TradingMode::Expert,"trend",&s,3000).unwrap();
        s.ready=true;sample(&db,"a",TradingMode::Expert,"trend",&s,4000).unwrap();
        let counts:(u64,u64)=db.query_row("SELECT checks,signals FROM brain_activity",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert_eq!(counts,(4,1));
        assert!(save(&db,"a",TradingMode::Expert,"system",Control::default()).is_err());
    }
    #[test] fn bulk_paper_is_atomic_account_mode_scoped_and_preserves_advisors() {
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        save(&db,"acct",TradingMode::Expert,"trend",Control{advisor:false,paper:true}).unwrap();
        let updated=set_all_paper(&db,"acct",TradingMode::Expert,Some(0),false).unwrap();
        assert_eq!(updated.len(),BRAINS.len()-1);
        assert!(updated.values().all(|c|!c.paper));
        assert!(!updated["trend"].advisor);
        assert!(load_account(&db,"acct",TradingMode::Expert,Some(2)).unwrap()["trend"].paper);
        assert!(load_account(&db,"other",TradingMode::Expert,Some(0)).unwrap().is_empty());
        assert!(load_account(&db,"acct",TradingMode::Sniper,Some(0)).unwrap().is_empty());
        let reopened=set_all_paper(&db,"acct",TradingMode::Expert,Some(0),true).unwrap();
        assert!(reopened.values().all(|c|c.paper));
        assert!(!reopened["trend"].advisor);
        assert!(set_all_paper(&db,"acct",TradingMode::Expert,None,false).is_err());
        db.execute_batch("CREATE TRIGGER stop_bulk BEFORE INSERT ON brain_controls WHEN NEW.brain='trend' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(set_all_paper(&db,"acct",TradingMode::Expert,Some(0),false).is_err());
        let after=load_account(&db,"acct",TradingMode::Expert,Some(0)).unwrap();
        assert!(after.values().all(|c|c.paper),"partial bulk changes must roll back");
    }
}
