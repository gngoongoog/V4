const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const src=fs.readFileSync(path.join(__dirname,'../src/terminal.tsx'),'utf8');
const binding=fs.readFileSync(path.join(__dirname,'../src/operations.tsx'),'utf8');
const local=src.match(/<div className="account-item account-switch">.*?<\/div><\/div>/s);
assert(local,'account header available');
assert(local[0].includes('DEMO متصل')&&local[0].includes('REAL متصل'));
assert(local[0].includes('إدارة الربط ↗'),'explicit action must navigate to binding');
assert(local[0].includes("onClick={()=>open('execution')}"));
assert(!/<button[^>]*>Live<\/button>|<button[^>]*>Demo<\/button>/.test(local[0]),
 'account header must not pretend its links switch MT5 broker sessions');
for(const term of ['إعداد Demo','إعداد Live','لا يغيّر الحساب مباشرة','لا يغيّر MT5 مباشرة']){
 assert(src.includes(term),'Demo/REAL dashboard labels must distinguish binding from switching: '+term);
}
assert(binding.includes("telemetry.account_trade_mode!==2||login.trim()!==String(telemetry.login)"),
 'REAL bind requires matching broker login typed explicitly');
assert(binding.includes("onBind(target,points,login.trim())"),
 'only confirmed binding form can request Core account switch');
console.log('PASS: Demo/REAL status is source truth; header and action controls navigate to explicit verified Core binding, never faux-switch MT5');
