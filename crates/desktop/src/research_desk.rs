use super::*;
pub fn setup(ui:&Dashboard)->slint::Timer {
    let(tx,rx)=mpsc::channel::<(String,Value)>();let sender=tx.clone();let weak=ui.as_weak();
    ui.on_desk_action(move|action|{
        let Some(ui)=weak.upgrade()else{return;};if ui.get_desk_busy(){return;}
        let action=action.to_string();
        let body=if action=="save"{
            let symbols=ui.get_correlation_symbols().split([',','،']).map(str::trim).filter(|v|!v.is_empty()).map(str::to_owned).collect::<Vec<_>>();
            json!({"enabled":ui.get_correlation_enabled(),"symbols":symbols,"timeframe":["M1","M5","M15","H1"][ui.get_correlation_frame().clamp(0,3) as usize]})
        }else if action=="replay"{json!({"at":ui.get_replay_at().trim(),"hours":ui.get_replay_hours().trim().parse::<u64>().unwrap_or(0)})}else{json!({})};
        ui.set_desk_busy(true);let _=sender.send((action,body));
    });
    let timer=slint::Timer::default();let weak=ui.as_weak();
    timer.start(slint::TimerMode::Repeated,Duration::from_secs(5),move||{
        if let Some(ui)=weak.upgrade(){if !ui.get_desk_busy()&&matches!(ui.get_page(),18|20){ui.set_desk_busy(true);let action=if ui.get_page()==18{"correlation-poll"}else{"ablation"};let _=tx.send((action.into(),json!({})));}}
    });
    let weak=ui.as_weak();std::thread::spawn(move||{
        let mut client=None;
        while let Ok((action,body))=rx.recv(){
            let result=(||->anyhow::Result<Value>{
                if client.is_none(){client=Some(Client::new(&data_dir())?);}
                let c=client.as_ref().unwrap();
                match action.as_str(){
                    "replay"=>c.post("/v1/news/replay",body),
                    "ablation"=>c.get("/v1/research-desk/ablation"),
                    "save"=>{c.post("/v1/research-desk/settings",body)?;c.get("/v1/research-desk/correlation")},
                    _=>c.get("/v1/research-desk/correlation")
                }
            })();
            if result.is_err(){client=None;}
            let _=weak.upgrade_in_event_loop(move|ui|{
                ui.set_desk_busy(false);
                match result{
                    Err(e)=>ui.set_desk_message(format!("تعذر الطلب: {e}").into()),
                    Ok(v)=>{
                        let text=v["text"].as_str().unwrap_or("");
                        if action=="replay"{ui.set_replay_text(text.into());}
                        else if action=="ablation"{ui.set_ablation_text(text.into());}
                        else{
                            ui.set_correlation_text(text.into());
                            if action!="correlation-poll"{
                                let s=&v["settings"];ui.set_correlation_enabled(s["enabled"]==true);
                                ui.set_correlation_symbols(s["symbols"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ").into());
                                ui.set_correlation_frame(["M1","M5","M15","H1"].iter().position(|tf|s["timeframe"]==*tf).unwrap_or(1) as i32);
                            }
                        }
                        ui.set_desk_message(if action=="save"{"حُفظت الإعدادات؛ القراءة التالية خلال نحو دقيقة"}else{"تم تحديث العرض"}.into());
                    }
                }
            });
        }
    });timer
}
