import { spawn, type ChildProcess } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';

const DEV_URL = 'http://localhost:1420';
const STARTUP_TIMEOUT_MS = 30_000;
const SHUTDOWN_TIMEOUT_MS = 5_000;

export class TauriDevHarness {
  private proc: ChildProcess | null = null;
  private stdoutBuffer: string[] = [];
  private stderrBuffer: string[] = [];

  async start(): Promise<void> {
    await this.assertPortFree();

    this.proc = spawn('bun', ['run', 'dev'], {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, ANSAMBEL_MOCK_CLAUDE: '1' },
      detached: process.platform !== 'win32'
    });

    this.drain(this.proc.stdout, this.stdoutBuffer);
    this.drain(this.proc.stderr, this.stderrBuffer);

    await this.waitForPort(DEV_URL, STARTUP_TIMEOUT_MS);
  }

  async stop(): Promise<void> {
    if (!this.proc || this.proc.killed) return;

    const proc = this.proc;
    const closePromise = new Promise<void>((resolve) => {
      proc.once('close', () => resolve());
    });

    if (process.platform === 'win32') {
      spawn('taskkill', ['/pid', String(proc.pid), '/t', '/f'], { stdio: 'ignore' });
    } else {
      try {
        process.kill(-proc.pid!, 'SIGTERM');
      } catch {
        proc.kill('SIGTERM');
      }
    }

    const killTimer = setTimeout(() => {
      if (!proc.killed) {
        if (process.platform === 'win32') {
          spawn('taskkill', ['/pid', String(proc.pid), '/t', '/f'], { stdio: 'ignore' });
        } else {
          try { process.kill(-proc.pid!, 'SIGKILL'); } catch { proc.kill('SIGKILL'); }
        }
      }
    }, SHUTDOWN_TIMEOUT_MS);

    await closePromise;
    clearTimeout(killTimer);
    this.proc = null;
  }

  /** Last N lines of stdout — useful for CI debugging on failure. */
  getStdoutTail(n = 50): string {
    return this.stdoutBuffer.slice(-n).join('');
  }

  /** Last N lines of stderr — useful for CI debugging on failure. */
  getStderrTail(n = 50): string {
    return this.stderrBuffer.slice(-n).join('');
  }

  private drain(stream: NodeJS.ReadableStream | null, buffer: string[]): void {
    if (!stream) return;
    stream.setEncoding('utf8');
    stream.on('data', (chunk: string) => {
      buffer.push(chunk);
      if (buffer.length > 500) buffer.splice(0, buffer.length - 500);
    });
  }

  private async assertPortFree(): Promise<void> {
    try {
      const res = await fetch(DEV_URL, { signal: AbortSignal.timeout(500) });
      if (res.ok) {
        throw new Error(
          `Port 1420 is already in use. A previous dev server may be stuck. Kill it (e.g. \`pkill -f "bun run dev"\`) and retry.`
        );
      }
    } catch (e) {
      if (e instanceof Error && e.message.includes('already in use')) throw e;
      // AbortError / ECONNREFUSED / TypeError are all fine — port is free
    }
  }

  private async waitForPort(url: string, timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const r = await fetch(url);
        if (r.ok) return;
      } catch {
        // keep polling
      }
      await sleep(300);
    }
    throw new Error(
      `Dev server did not start at ${url} within ${timeoutMs}ms. stderr tail:\n${this.getStderrTail(30)}`
    );
  }
}
