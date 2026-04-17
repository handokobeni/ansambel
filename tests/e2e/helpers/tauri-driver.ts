import { spawn, type ChildProcess } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';

export class TauriDevHarness {
  private proc: ChildProcess | null = null;
  async start(): Promise<void> {
    this.proc = spawn('bun', ['run', 'dev'], {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, ANSAMBEL_MOCK_CLAUDE: '1' }
    });
    await this.waitForPort('http://localhost:1420', 30_000);
  }

  async stop(): Promise<void> {
    if (this.proc && !this.proc.killed) {
      this.proc.kill();
      await new Promise((r) => this.proc!.once('close', r));
    }
  }

  private async waitForPort(url: string, timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const r = await fetch(url);
        if (r.ok) return;
      } catch {}
      await sleep(300);
    }
    throw new Error(`Dev server did not start at ${url} within ${timeoutMs}ms`);
  }
}
