import type {Status} from './types';

// Separate a live Core heartbeat from *actual* broker data freshness. A 2s
// Tauri heartbeat cannot turn an old quote into a new market sample.
export type FeedEvidence={core:boolean;broker:boolean;quote:boolean;quoteAgeMs:number|null};
export function feedEvidence(status:Status|null,online:boolean,lastStatusAt:number|null,now:number):FeedEvidence{
 const core=Boolean(status&&online&&lastStatusAt!==null&&now>=lastStatusAt&&now-lastStatusAt<=5000);
 const broker=Boolean(core&&status?.gateway_fresh===true&&status?.telemetry?.connected===true);
 const age=status?.telemetry?.tick_age_ms;
 const received=status?.last_received_ms;
 let quoteAgeMs:number|null=null;
 if(broker&&typeof age==='number'&&Number.isFinite(age)&&age>=0&&
    typeof received==='number'&&Number.isFinite(received)&&received>0&&received<=now+2000){
   // tick_age_ms belongs to the gateway snapshot at last_received_ms.
   // The elapsed time since that snapshot must be added, even when Core
   // responds frequently with an unchanged MT5 quote.
   quoteAgeMs=age+Math.max(0,now-received);
 }
 return {core,broker,quote:broker&&quoteAgeMs!==null&&quoteAgeMs<=3000,quoteAgeMs};
}

/** Connection-first operator status. A cached Core message is not broker
 * connectivity evidence, even if it once said that MT5 was connected. */
export function connectionStatusMessage(status:Status|null,feed:FeedEvidence,preview:boolean):string{
 if(preview)return 'معاينة التصميم فقط — لا اتصال أو تنفيذ MT5 حقيقي';
 if(!feed.core)return 'Rust Core غير متاح أو انتهت مهلة تحديثه؛ حالة اتصال MT5 غير مؤكدة';
 if(!feed.broker)return 'Rust Core متصل، لكن اتصال MT5 غير مؤكد أو لقطة الحساب قديمة — لا تعتمد على آخر رسالة اتصال';
 if(!feed.quote)return 'حساب MT5 متصل، لكن آخر سعر غير حديث أو غير متاح — التداول بسعر قديم ممنوع';
 return status?.message?.trim()||'اتصال Core وحساب MT5 والسعر حديث ومؤكد';
}
