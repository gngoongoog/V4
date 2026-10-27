/** Read-only transport fence for concurrent Core dashboard queries.
 * Changes here affect presentation only, NEVER broker commands.
 */
export type ScopedReadTicket = Readonly<{scope:string;channel:string;revision:number}>;

export class ScopedReadFence {
  private scope='';
  private sequence=0;
  private latest=new Map<string,number>();

  setScope(next:string):void {
    if(next!==this.scope){
      this.scope=next;
      this.latest.clear(); // Also rejects account A -> B -> A completions.
    }
  }

  beginRead(channel:string):ScopedReadTicket {
    if(!channel)throw new Error('Missing Core read channel');
    const revision=++this.sequence;
    this.latest.set(channel,revision);
    return {scope:this.scope,channel,revision};
  }

  invalidate(channel:string):void {
    this.latest.delete(channel);
  }

  isCurrent(ticket:ScopedReadTicket):boolean {
    return Boolean(ticket&&ticket.scope===this.scope&&
      Number.isSafeInteger(ticket.revision)&&ticket.revision>0&&
      this.latest.get(ticket.channel)===ticket.revision);
  }
}
