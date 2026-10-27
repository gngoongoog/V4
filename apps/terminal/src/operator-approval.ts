import type {Status} from './types';

export type OperatorAction='start'|'stop'|'exit';
export type OperatorApproval={action:OperatorAction;scope:string};

/**
 * Operator consent binds to one observed broker/account/connection scope.
 * This is a UI preflight only. Rust Core and its durable execution gates
 * remain the authority on every subsequent broker action.
 */
export function approvalIsCurrent(
 approval:OperatorApproval|null,
 action:OperatorAction,
 currentScope:string
):boolean{
 return approval!==null&&approval.action===action&&
        approval.scope===currentScope&&currentScope.length>0;
}

export function canConfirmStart(
 approval:OperatorApproval|null,
 currentScope:string,
 status:Status|null,
 brokerFresh:boolean,
 enteredLiveLogin:string
):boolean{
 if(!approvalIsCurrent(approval,'start',currentScope)||
    !brokerFresh||status?.gateway_fresh!==true||
    status.requested_run===true)return false;
 const t=status.telemetry;
 if(!t||t.connected!==true||!Number.isSafeInteger(t.login)||t.login<=0||
    !t.server||!t.symbol||!t.currency)return false;
 // An unverified account_trade_mode must never be classified as Demo or REAL.
 if(t.demo===true&&t.account_trade_mode===0)return enteredLiveLogin.trim()==='';
 if(t.demo===false&&t.account_trade_mode===2)
   return enteredLiveLogin.trim()===String(t.login);
 return false;
}
