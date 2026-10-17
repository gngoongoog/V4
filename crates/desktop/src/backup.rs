//! Native backup/restore and portable Brain Snapshot controls. Core validates again before staging.
use super::*;
use anyhow::{bail, Context};
use slint::Model;
use std::{fs, path::PathBuf};

fn export_brain(client:&Client)->anyhow::Result<String>{
    let v=client.post("/v1/brain-snapshot/export",json!({}))?;
    let source=PathBuf::from(v["path"].as_str().context("مسار ملف العقل غير موجود")?);
    let file_name=v["file_name"].as_str().unwrap_or("BrainTrade-Brain.btbrain");
    let Some(destination)=rfd::FileDialog::new().add_filter("BrainTrade Brain Snapshot",&["btbrain"]).set_file_name(file_name).save_file() else{let _=fs::remove_file(&source);return Ok("أُلغي تصدير العقل".into());};
    fs::copy(&source,&destination)?;let _=fs::remove_file(&source);
    Ok(format!("تم حفظ Brain Snapshot بنجاح:\n{}\nChecksum: {}\nيحتوي التعلم والنماذج ونتائج Broker/Studio، ولا يحتوي أسرار Telegram أو journal الإرسال.",destination.display(),v["checksum"].as_str().unwrap_or("—")))
}

fn import_brain(client:&Client)->anyhow::Result<(String,bool)>{
    if client.get("/v1/status")?["state"]!="stopped"{bail!("اضغط STOP قبل استيراد Brain Snapshot");}
    let Some(source)=rfd::FileDialog::new().add_filter("BrainTrade Brain Snapshot",&["btbrain"]).pick_file() else{return Ok(("أُلغي استيراد العقل".into(),false));};
    let inbox=data_dir().join("brain_imports");fs::create_dir_all(&inbox)?;
    let inbox_name=format!("desktop-{}.btbrain",braintrade_runtime::now_ms());let inbox_path=inbox.join(&inbox_name);fs::copy(&source,&inbox_path)?;
    let imported=match client.post("/v1/brain-snapshot/import",json!({"name":inbox_name})) {Ok(v)=>v,Err(e)=>{let _=fs::remove_file(&inbox_path);return Err(e);}};
    let backup=imported["backup_name"].as_str().context("لم يُرجع Core اسم نقطة الاستعادة")?.to_owned();
    let answer=rfd::MessageDialog::new().set_title("استيراد Brain Snapshot")
        .set_description(format!("تم فحص الملف بنجاح.\nنقطة الاستعادة: {backup}\n\nسيتم استرجاع التعلم، أوزان العقول الناتجة من الأدلة، نماذج التعلم، Strategy Studio والبيانات البحثية.\nلا تُسترجع أسرار Telegram ولا journal إرسال الأوامر.\nللاستمرار الدقيق بأوزان Coordinator استخدم نفس حساب/Server/Symbol.\n\nسيُغلق التطبيق بعد تجهيز الاستعادة. هل تريد المتابعة؟"))
        .set_level(rfd::MessageLevel::Warning).set_buttons(rfd::MessageButtons::YesNo).show();
    if answer!=rfd::MessageDialogResult::Yes{return Ok((format!("تم استيراد الملف وفحصه فقط. يمكنك استعادة {backup} لاحقاً من قائمة النسخ."),false));}
    let staged=client.post("/v1/backup/restore/stage",json!({"name":backup}))?;
    if staged["staged"]!=true{bail!("تم استيراد الملف لكن لم تُجهز الاستعادة");}
    rfd::MessageDialog::new().set_title("Brain Snapshot جاهز")
        .set_description("تم تجهيز استعادة العقل. سيتوقف التطبيق؛ افتح BrainTrade.exe من جديد ليعود التعلم من النقطة المحفوظة.").show();
    Ok((String::new(),true))
}

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
                "brain_export"=>{message=export_brain(&client)?;},
                "brain_import"=>{let(m,exit)=import_brain(&client)?;if exit{return Ok((Value::Null,String::new(),true));}message=m;},
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
