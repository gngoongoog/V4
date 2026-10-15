//! Native backup/restore controls. Core validates again before staging.
use super::*;
use anyhow::{bail, Context};
use slint::Model;

pub fn setup(ui: &Dashboard) {
    let (tx,rx)=mpsc::channel::<(String,String)>();
    let weak=ui.as_weak();
    ui.on_backup_action(move |action| {
        if let Some(ui)=weak.upgrade() {
            if ui.get_backup_busy(){return;}
            let selected=ui.get_backup_names().row_data(ui.get_backup_selected().max(0) as usize).unwrap_or_default();
            ui.set_backup_busy(true);
            let _=tx.send((action.to_string(),selected.to_string()));
        }
    });
    let weak=ui.as_weak();
    std::thread::spawn(move ||{while let Ok((action,name))=rx.recv(){
        let result=(||->anyhow::Result<(Value,String,bool)>{
            let client=Client::new(&data_dir()).context("المحرك غير متاح")?;
            let mut message=String::new();
            match action.as_str(){
                "refresh"=>{},
                "create"=>{let v=client.post("/v1/backup",json!({}))?;message=format!("تم إنشاء نسخة: {}",v["name"].as_str().unwrap_or("—"));},
                "validate"|"restore"=>{
                    if name.is_empty(){bail!("اختر نسخة احتياطية أولاً");}
                    let checked=client.post("/v1/backup/restore/validate",json!({"name":name}))?;
                    message=serde_json::to_string_pretty(&checked)?;
                    if action=="restore" {
                        if client.get("/v1/status")?["state"]!="stopped"{bail!("اضغط إيقاف النظام قبل الاستعادة");}
                        let answer=rfd::MessageDialog::new().set_title("تأكيد استعادة النسخة الاحتياطية")
                            .set_description(format!("النسخة: {name}\nستُستبدل الإعدادات والسجل والبيانات البحثية. تُحفظ نسخة تراجع تلقائياً.\nأسرار Telegram المحلية لا تتغير.\nسيُغلق التطبيق؛ افتح BrainTrade.exe مجدداً لتطبيق الاستعادة. هل تريد المتابعة؟"))
                            .set_level(rfd::MessageLevel::Warning).set_buttons(rfd::MessageButtons::YesNo).show();
                        if answer!=rfd::MessageDialogResult::Yes {return Ok((client.get("/v1/backups")?,"أُلغيت الاستعادة".into(),false));}
                        let staged=client.post("/v1/backup/restore/stage",json!({"name":name}))?;
                        if staged["staged"]!=true{bail!("لم تُجهز الاستعادة؛ راجع السجل");}
                        rfd::MessageDialog::new().set_title("الاستعادة جاهزة")
                            .set_description("أُوقِف المحرك. افتح BrainTrade.exe مجدداً لتطبيق النسخة؛ راجع نتيجة الاستعادة في صفحة النسخ الاحتياطية.").show();
                        return Ok((Value::Null,String::new(),true));
                    }
                },
                _=>bail!("Unknown backup action"),
            }
            let last=client.get("/v1/backup/restore/status")?;
            Ok((client.get("/v1/backups")?,format!("{message}\n\nحالة الاستعادة ونتيجة آخر محاولة:\n{}",serde_json::to_string_pretty(&last)?),false))
        })();
        let selected=name.clone();
        let _=weak.upgrade_in_event_loop(move |ui|{
            ui.set_backup_busy(false);
            match result {
                Ok((_,_,true))=>{let _=slint::quit_event_loop();},
                Ok((list,message,false))=>{
                    let rows=list["backups"].as_array().cloned().unwrap_or_default();
                    let names:Vec<slint::SharedString>=rows.iter().filter_map(|r|r["name"].as_str().map(Into::into)).collect();
                    let index=names.iter().position(|n|n.as_str()==selected).unwrap_or(0);
                    let mut details=String::new();
                    for row in &rows{details.push_str(&format!("{}\n{}\n\n",row["name"].as_str().unwrap_or("—"),if row["valid"]==true{"صالحة للاستعادة"}else{row["error"].as_str().unwrap_or("غير صالحة")}));}
                    if names.is_empty(){details.push_str("لا توجد نسخ احتياطية بعد. أنشئ نسخة أولاً.\n");}
                    ui.set_backup_names(slint::ModelRc::new(slint::VecModel::from(names)));
                    ui.set_backup_selected(index as i32);
                    ui.set_backup_detail(format!("{details}\n{message}").into());
                },
                Err(e)=>ui.set_backup_detail(format!("تعذرت العملية: {e}").into()),
            }
        });
    }});
}
