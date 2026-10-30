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
// Paths are chosen by native file dialogs, never supplied by the renderer.
export async function exportBrainSnapshot():Promise<unknown>{return invoke('brain_snapshot_export')}
export async function importBrainSnapshot():Promise<unknown>{return invoke('brain_snapshot_import')}
export async function restoreBrainSnapshot(name:string):Promise<unknown>{return invoke('brain_snapshot_restore',{name})}
export async function saveExcelReport(bytes:Uint8Array,kind:'journal'|'report'):Promise<unknown>{return invoke('save_excel_report',{bytes:Array.from(bytes),kind})}
export async function backtestAction(action:'start'|'cancel',request?:Record<string,unknown>):Promise<unknown>{return invoke('backtest_action',{action,request:request??null})}
export async function updateBrainControl(context:string,brain:string,mode:string,advisor:boolean,paper:boolean):Promise<unknown>{return invoke('update_brain_control',{context,brain,mode,advisor,paper})}
export async function updateBrainPaperAll(context:string,mode:string,paper:boolean):Promise<unknown>{return invoke('update_brain_paper_all',{context,mode,paper})}
export type RiskRecovery={basis:'balance'|'equity';threshold:number};
export async function updateRiskMaster(context:string,enabled:boolean,confirmLiveLogin:string,recovery:RiskRecovery|null):Promise<unknown>{return invoke('update_risk_master',{context,enabled,confirmLiveLogin,recovery})}
export type PositionProtectionPolicy={break_even_enabled:boolean;break_even_trigger_points:number;break_even_offset_points:number;trailing_enabled:boolean;trailing_distance_points:number;trailing_step_points:number};
export async function updatePositionProtection(context:string,policy:PositionProtectionPolicy,expectedPolicyId:string,confirmLiveLogin:string):Promise<unknown>{return invoke('update_position_protection',{context,policy,expectedPolicyId,confirmLiveLogin})}
export async function setLearningFrozen(frozen:boolean):Promise<unknown>{return invoke('set_learning_frozen',{frozen})}
export async function manualMarket(request:Record<string,unknown>):Promise<unknown>{return invoke('manual_market',{request})}
export async function selectSoloBrain(context:string,mode:string,brain:string):Promise<unknown>{return invoke('select_solo_brain',{context,mode,brain})}
export type SelectedClosePosition={ticket:number;magic:number;side:string;volume:number};
export async function closeOwnedPositions(context:string,positions:SelectedClosePosition[],profitOnly:boolean):Promise<unknown>{return invoke('close_owned_positions',{context,positions,profitOnly})}
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
export async function frozenCoordinatorAction(action:'freeze'|'seal_candidate'|'stop',context:string,mode:'expert'|'scalping'|'swing'):Promise<unknown>{
 return invoke('frozen_coordinator_action',{action,context,mode});
}
