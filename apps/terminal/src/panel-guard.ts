/** Replayed into apps/terminal/src/panel-guard.ts at Windows build time.
 * Presentation-only read ordering fence; never authorizes execution.
 */
export class PanelReadFence {
  private scope = '';
  private revision = 0;

  setScope(next: string): void {
    if (next !== this.scope) {
      this.scope = next;
      this.revision += 1;
    }
  }

  beginRead(): number {
    this.revision += 1;
    return this.revision;
  }

  isCurrent(revision: number): boolean {
    return Number.isSafeInteger(revision) && revision === this.revision;
  }
}
