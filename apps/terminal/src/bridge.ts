import {invoke,Channel} from '@tauri-apps/api/core';
import type {Status,StreamEvent} from './types';
export const hasTauri=()=>typeof window!=='undefined'&&('__TAURI_INTERNALS__' in window);
export async function coreStatus():Promise<Status>{return invoke<Status>('get_status')}
export async function readPanel(route:string):Promise<unknown>{return invoke('read_panel',{route})}
// Exact read-only native pagination: no arbitrary HTTP methods or destinations.
export async function readTradeHistory(offset:number,limit:number):Promise<unknown>{
 if(!Number.isSafeInteger(offset)||offset<0||offset>1000000||
    !Number.isSafeInteger(limit)||limit<1||limit>100)
  throw new Error('Invalid Core trade-history pagination');
 return invoke('read_trade_history',{offset,limit});
}
export async function control(action:'start'|'stop'|'reconnect',confirmLiveLogin?:string):Promise<unknown>{return invoke('execute_control',{action,confirmLiveLogin:confirmLiveLogin??null})}
export async function saveRisk(risk:Record<string,unknown>):Promise<unknown>{return invoke('update_risk',{risk})}
export async function savePortfolio(settings:Record<string,unknown>):Promise<unknown>{return invoke('update_portfolio',{settings})}
export async function saveSettings(config:Record<string,unknown>,telegramToken='',clearTelegramToken=false):Promise<unknown>{return invoke('update_settings',{config,telegramToken,clearTelegramToken})}
export async function saveTrading(settings:Record<string,unknown>):Promise<unknown>{return invoke('update_trading_settings',{settings})}
export async function handoff(sniperActive:boolean):Promise<unknown>{return invoke('priority_handoff',{sniperActive})}
export async function shutdownCore():Promise<unknown>{return invoke('shutdown_core')}
export async function closeTerminal():Promise<void>{return invoke('close_terminal')}
export async function backupAction(action:'create'|'brain_export'):Promise<unknown>{return invoke('backup_action',{action})}
export async function backtestAction(action:'start'|'cancel',request?:Record<string,unknown>):Promise<unknown>{return invoke('backtest_action',{action,request:request??null})}
export async function updateBrainControl(context:string,brain:string,mode:string,advisor:boolean,paper:boolean):Promise<unknown>{return invoke('update_brain_control',{context,brain,mode,advisor,paper})}
export async function bindExecution(target:'demo'|'live',deviationPoints:number,confirmLiveLogin:string):Promise<unknown>{return invoke('bind_execution',{target,deviationPoints,confirmLiveLogin})}
// Channel is one-way backend -> renderer, and only carries trusted Core status.
export async function subscribe(onChange:(update:StreamEvent)=>void):Promise<void>{
  const stream=new Channel<StreamEvent>();
  stream.onmessage=onChange;
  await invoke('subscribe_status',{channel:stream});
}

export async function saveSupervisor(settings:Record<string,unknown>):Promise<unknown>{return invoke('update_supervisor',{settings})}
export async function frozenSniperAction(action:'freeze'|'stop',context:string):Promise<unknown>{
 return invoke('frozen_sniper_action',{action,context});
}
export async function frozenClassicAction(action:'freeze'|'stop',context:string):Promise<unknown>{
 return invoke('frozen_classic_action',{action,context});
}
export async function frozenCoordinatorAction(action:'freeze'|'stop',context:string,mode:'expert'|'scalping'|'swing'):Promise<unknown>{
 return invoke('frozen_coordinator_action',{action,context,mode});
}
