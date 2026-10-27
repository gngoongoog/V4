/** Presentation-level non-reentrant mutation fence.
 *
 * Do not mistake this UI fence for cancellation or broker authorization.
 * Once an IPC write was sent, Rust Core MAY have committed it even if the
 * user switched accounts. Keep new writes blocked until the old one settles;
 * never publish stale success, edit data or follow-up reads to a new scope.
 */
export type OperatorWriteTicket=Readonly<{scope:string;epoch:number;action:string}>;

export class OperatorWriteFence {
 private scope='';
 private epoch=0;
 private pending:OperatorWriteTicket|null=null;

 setScope(next:string):void {
  if(next!==this.scope){
   this.scope=next;
   this.epoch++; // A -> B -> A must still invalidate old writes.
  }
 }

 begin(action:string):OperatorWriteTicket|null {
  if(!action||this.pending)return null;
  const ticket={scope:this.scope,epoch:++this.epoch,action};
  this.pending=ticket;
  return ticket;
 }

 isCurrent(ticket:OperatorWriteTicket|null):boolean {
  return ticket!==null&&ticket===this.pending&&
   ticket.scope===this.scope&&ticket.epoch===this.epoch;
 }

 finish(ticket:OperatorWriteTicket|null):boolean {
  if(ticket===null||ticket!==this.pending)return false;
  this.pending=null;
  return true;
 }

 get busy():boolean{return this.pending!==null}
}
