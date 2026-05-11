use crate::error::{AppError, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};

const DEFAULT_ROWS: u16 = 30;
const DEFAULT_COLS: u16 = 120;

/// PTY abstraction. The production impl is `PortablePty` (wraps the
/// `portable-pty` crate); tests use `MockPty` for deterministic,
/// OS-independent behavior. Code that orchestrates terminal sessions
/// depends only on this trait.
pub trait Pty: Send + std::fmt::Debug {
    fn pid(&self) -> u32;
    fn reader(&self) -> Result<Box<dyn Read + Send>>;
    fn writer(&self) -> Result<Box<dyn Write + Send>>;
    fn kill(&mut self) -> Result<()>;
    /// Drop the master PTY handle. On Windows ConPTY this is what
    /// propagates EOF to outstanding readers — `child.kill()` alone does
    /// not. Idempotent.
    fn close_master(&mut self);
    fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>>;
    fn resize(&self, rows: u16, cols: u16) -> Result<()>;
}

// ── PortablePty: production impl backed by the `portable-pty` crate ─

pub struct PortablePty {
    // `Option<>` so `close_master()` can drop the master to propagate EOF
    // to readers on Windows ConPTY. On Linux dropping is unnecessary but
    // harmless; on Windows a clean child exit does not always EOF the
    // master, so the watchdog explicitly drops it.
    master: Option<Box<dyn MasterPty + Send>>,
    child: Box<dyn Child + Send + Sync>,
    pid: u32,
}

impl std::fmt::Debug for PortablePty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortablePty")
            .field("pid", &self.pid)
            .finish()
    }
}

pub fn spawn(cmd: CommandBuilder) -> Result<PortablePty> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Command {
            cmd: "openpty".into(),
            msg: e.to_string(),
        })?;

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::Command {
            cmd: "spawn_command".into(),
            msg: e.to_string(),
        })?;

    // IMPORTANT: close slave in parent so child receives EOF when writer is dropped.
    drop(pair.slave);

    let pid = child.process_id().unwrap_or(0);
    Ok(PortablePty {
        master: Some(pair.master),
        child,
        pid,
    })
}

impl Pty for PortablePty {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn reader(&self) -> Result<Box<dyn Read + Send>> {
        self.master
            .as_ref()
            .ok_or_else(|| AppError::Command {
                cmd: "pty.reader".into(),
                msg: "master pty closed".into(),
            })?
            .try_clone_reader()
            .map_err(|e| AppError::Command {
                cmd: "pty.reader".into(),
                msg: e.to_string(),
            })
    }

    fn writer(&self) -> Result<Box<dyn Write + Send>> {
        self.master
            .as_ref()
            .ok_or_else(|| AppError::Command {
                cmd: "pty.writer".into(),
                msg: "master pty closed".into(),
            })?
            .take_writer()
            .map_err(|e| AppError::Command {
                cmd: "pty.writer".into(),
                msg: e.to_string(),
            })
    }

    fn kill(&mut self) -> Result<()> {
        self.child.kill().map_err(|e| AppError::Command {
            cmd: "kill".into(),
            msg: e.to_string(),
        })?;
        Ok(())
    }

    fn close_master(&mut self) {
        self.master = None;
    }

    fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>> {
        self.child.try_wait().map_err(|e| AppError::Command {
            cmd: "try_wait".into(),
            msg: e.to_string(),
        })
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master
            .as_ref()
            .ok_or_else(|| AppError::Command {
                cmd: "resize".into(),
                msg: "master pty closed".into(),
            })?
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::Command {
                cmd: "resize".into(),
                msg: e.to_string(),
            })?;
        Ok(())
    }
}

// ── MockPty: in-memory impl for unit tests ──────────────────────────
//
// `MockPty` and its `MockPtyHandle` give tests full deterministic
// control over a fake PTY without spawning any process. The reader is
// fed by a channel the test pushes to via the handle; the writer
// collects bytes into a buffer the test can inspect; `try_wait` returns
// whatever exit status the handle sets; `close_master` and `kill`
// signal EOF to the reader so the reader thread terminates and the
// surrounding state machine (watchdog, exit chunk) fires.
//
// Public (not `#[cfg(test)]`) so other modules' integration tests can
// import it without re-implementing the plumbing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct MockPty {
    pid: u32,
    inner: Arc<MockPtyInner>,
}

#[derive(Debug)]
struct MockPtyInner {
    // Reader side: a channel the handle pushes bytes to. The reader
    // returned by `reader()` calls `rx.recv()` blocking. When all
    // senders are dropped (handle drops + close_master clears `tx`),
    // `recv` errors and the reader returns Ok(0) — EOF.
    stdout_tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    stdout_rx: Mutex<Option<mpsc::Receiver<Vec<u8>>>>,
    // Writer side: every byte written is appended here. Tests can
    // assert against the cumulative input.
    stdin_buf: Mutex<Vec<u8>>,
    // try_wait state. None = still running. Some(status) = exited.
    exit_status: Mutex<Option<portable_pty::ExitStatus>>,
    // Closed flag for resize() — once close_master() has been called,
    // resize errors just like the real impl.
    closed: AtomicBool,
    // Last resize call, for assertion. Real impl ignores this; tests
    // can verify resize was forwarded.
    last_resize: Mutex<Option<(u16, u16)>>,
}

/// Test-side handle. Use to push stdout, inspect stdin, and trigger
/// exit. Cloning is cheap (Arc-backed).
#[derive(Clone, Debug)]
pub struct MockPtyHandle {
    inner: Arc<MockPtyInner>,
}

impl MockPty {
    pub fn new(pid: u32) -> (Self, MockPtyHandle) {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let inner = Arc::new(MockPtyInner {
            stdout_tx: Mutex::new(Some(tx)),
            stdout_rx: Mutex::new(Some(rx)),
            stdin_buf: Mutex::new(Vec::new()),
            exit_status: Mutex::new(None),
            closed: AtomicBool::new(false),
            last_resize: Mutex::new(None),
        });
        let mock = MockPty {
            pid,
            inner: Arc::clone(&inner),
        };
        let handle = MockPtyHandle { inner };
        (mock, handle)
    }
}

impl MockPtyHandle {
    /// Push bytes onto the reader's channel. The reader thread will
    /// receive these on its next `read()` call.
    pub fn push_stdout(&self, bytes: &[u8]) {
        if let Ok(guard) = self.inner.stdout_tx.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(bytes.to_vec());
            }
        }
    }

    /// Inspect the cumulative bytes the writer has received.
    pub fn stdin_bytes(&self) -> Vec<u8> {
        self.inner
            .stdin_buf
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Set the exit status. Subsequent `try_wait()` calls return this.
    pub fn set_exited(&self, code: u32) {
        if let Ok(mut g) = self.inner.exit_status.lock() {
            *g = Some(portable_pty::ExitStatus::with_exit_code(code));
        }
    }

    /// Closes the stdout channel so the reader sees EOF. Idempotent.
    pub fn close_stdout(&self) {
        if let Ok(mut g) = self.inner.stdout_tx.lock() {
            *g = None;
        }
    }

    pub fn last_resize(&self) -> Option<(u16, u16)> {
        self.inner.last_resize.lock().ok().and_then(|g| *g)
    }
}

impl Pty for MockPty {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn reader(&self) -> Result<Box<dyn Read + Send>> {
        let rx = self
            .inner
            .stdout_rx
            .lock()
            .map_err(|e| AppError::Other(e.to_string()))?
            .take()
            .ok_or_else(|| AppError::Command {
                cmd: "mock.reader".into(),
                msg: "reader already taken".into(),
            })?;
        Ok(Box::new(MockReader {
            rx,
            leftover: Vec::new(),
        }))
    }

    fn writer(&self) -> Result<Box<dyn Write + Send>> {
        Ok(Box::new(MockWriter {
            buf: Arc::clone(&self.inner),
        }))
    }

    fn kill(&mut self) -> Result<()> {
        // Killing in the mock = mark exited with code 137 (SIGKILL-ish)
        // and close stdout so any blocked reader wakes up.
        if let Ok(mut g) = self.inner.exit_status.lock() {
            if g.is_none() {
                *g = Some(portable_pty::ExitStatus::with_exit_code(137));
            }
        }
        if let Ok(mut g) = self.inner.stdout_tx.lock() {
            *g = None;
        }
        Ok(())
    }

    fn close_master(&mut self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        if let Ok(mut g) = self.inner.stdout_tx.lock() {
            *g = None;
        }
    }

    fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>> {
        Ok(self.inner.exit_status.lock().ok().and_then(|g| g.clone()))
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(AppError::Command {
                cmd: "resize".into(),
                msg: "master pty closed".into(),
            });
        }
        if let Ok(mut g) = self.inner.last_resize.lock() {
            *g = Some((rows, cols));
        }
        Ok(())
    }
}

struct MockReader {
    rx: mpsc::Receiver<Vec<u8>>,
    leftover: Vec<u8>,
}

impl Read for MockReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.leftover.is_empty() {
            let n = buf.len().min(self.leftover.len());
            buf[..n].copy_from_slice(&self.leftover[..n]);
            self.leftover.drain(..n);
            return Ok(n);
        }
        match self.rx.recv() {
            Ok(chunk) => {
                let n = buf.len().min(chunk.len());
                buf[..n].copy_from_slice(&chunk[..n]);
                if n < chunk.len() {
                    self.leftover.extend_from_slice(&chunk[n..]);
                }
                Ok(n)
            }
            // All senders dropped → EOF.
            Err(_) => Ok(0),
        }
    }
}

struct MockWriter {
    buf: Arc<MockPtyInner>,
}

impl Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut g) = self.buf.stdin_buf.lock() {
            g.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    fn echo_command() -> CommandBuilder {
        let mut cmd = if cfg!(windows) {
            let mut c = CommandBuilder::new("cmd");
            c.args(["/C", "echo hello"]);
            c
        } else {
            let mut c = CommandBuilder::new("sh");
            c.args(["-c", "echo hello"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        cmd
    }

    #[test]
    fn spawn_pty_returns_session() {
        let session = spawn(echo_command()).expect("spawn echo");
        assert!(session.pid() > 0);
    }

    #[test]
    fn spawn_pty_reads_stdout() {
        let session = spawn(echo_command()).expect("spawn echo");
        let reader = session.reader().expect("clone reader");
        let mut buf = String::new();
        let mut br = BufReader::new(reader);
        br.read_line(&mut buf).expect("read line");
        assert!(buf.contains("hello"), "got: {buf:?}");
    }

    #[test]
    fn spawn_pty_writes_stdin() {
        // On Windows: ConPTY processes pipe-writes asynchronously through a
        // VT-sequence translation layer.  The roundtrip (write → child reads →
        // child echoes) is inherently racy and has proved unreliable across CI
        // runners.  We verify only that writer() succeeds and write_all/flush
        // return Ok — the functional roundtrip is covered on Unix below.
        #[cfg(windows)]
        {
            let cmd = {
                let mut c = CommandBuilder::new("cmd");
                c.args(["/C", "echo hello"]);
                c.cwd(std::env::temp_dir());
                c
            };
            let session = spawn(cmd).expect("spawn");
            let mut writer = session.writer().expect("writer");
            assert!(writer.write_all(b"world\r\n").is_ok());
            assert!(writer.flush().is_ok());
        }
        #[cfg(not(windows))]
        {
            let cmd = {
                let mut c = CommandBuilder::new("sh");
                c.args(["-c", "read X; echo got=$X"]);
                c.cwd(std::env::temp_dir());
                c
            };
            let session = spawn(cmd).expect("spawn");
            let reader = session.reader().expect("reader");
            let mut writer = session.writer().expect("writer");
            // Unix PTY buffers stdin so timing is not critical; read in a
            // thread to guard against an unlikely EOF-delivery delay.
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            std::thread::spawn(move || {
                let mut br = BufReader::new(reader);
                let mut out = String::new();
                for _ in 0..20 {
                    let mut line = String::new();
                    match br.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    out.push_str(&line);
                    if out.contains("got=world") {
                        break;
                    }
                }
                let _ = tx.send(out);
            });
            writeln!(writer, "world").expect("write");
            writer.flush().expect("flush");
            drop(writer);
            let out = rx.recv_timeout(Duration::from_secs(10)).unwrap_or_default();
            assert!(
                out.contains("got=world"),
                "expected got=world in PTY output, got: {out:?}"
            );
        }
    }

    #[test]
    fn pty_session_pid_is_stable() {
        let session = spawn(echo_command()).expect("spawn echo");
        let pid_a = session.pid();
        let pid_b = session.pid();
        assert_eq!(pid_a, pid_b);
    }

    #[test]
    fn pty_session_kill_terminates_child() {
        let mut cmd = if cfg!(windows) {
            let mut c = CommandBuilder::new("cmd");
            c.args(["/C", "ping -n 60 127.0.0.1"]);
            c
        } else {
            let mut c = CommandBuilder::new("sh");
            c.args(["-c", "sleep 60"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        let mut session = spawn(cmd).expect("spawn sleep");
        std::thread::sleep(Duration::from_millis(100));
        session.kill().expect("kill");
        for _ in 0..40 {
            if session.try_wait().expect("try_wait").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("child did not exit within 2s of kill");
    }

    #[test]
    fn close_master_drops_handle_and_errors_subsequent_reader() {
        let session = spawn(echo_command()).expect("spawn echo");
        // First reader: master still open.
        assert!(session.reader().is_ok());
        let mut session = session;
        session.close_master();
        // After close, reader/writer/resize must return an error rather
        // than panic. `kill()` still works because it talks to `child`,
        // not `master`.
        let r = session.reader();
        assert!(r.is_err(), "reader should err after close_master");
        let w = session.writer();
        assert!(w.is_err(), "writer should err after close_master");
        let r2 = session.resize(24, 80);
        assert!(r2.is_err(), "resize should err after close_master");
        // Idempotent: calling close_master a second time is fine.
        session.close_master();
    }

    #[test]
    fn spawn_pty_unknown_binary_returns_err() {
        let cmd = CommandBuilder::new("definitely-not-a-real-binary-xyz");
        let result = spawn(cmd);
        assert!(result.is_err());
    }

    #[test]
    fn pty_session_resize_succeeds() {
        let session = spawn(echo_command()).expect("spawn echo");
        assert!(session.resize(40, 80).is_ok());
    }

    #[test]
    fn pty_session_resize_different_sizes() {
        let session = spawn(echo_command()).expect("spawn echo");
        assert!(session.resize(24, 80).is_ok());
        assert!(session.resize(50, 220).is_ok());
        assert!(session.resize(DEFAULT_ROWS, DEFAULT_COLS).is_ok());
    }

    #[test]
    fn pty_session_try_wait_returns_ok() {
        let mut cmd = if cfg!(windows) {
            let mut c = CommandBuilder::new("cmd");
            c.args(["/C", "exit 0"]);
            c
        } else {
            let mut c = CommandBuilder::new("sh");
            c.args(["-c", "exit 0"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        let mut session = spawn(cmd).expect("spawn exit");
        // Give the child time to exit.
        std::thread::sleep(Duration::from_millis(200));
        let result = session.try_wait();
        assert!(result.is_ok());
    }

    #[test]
    fn pty_session_writer_can_write() {
        let mut cmd = if cfg!(windows) {
            let mut c = CommandBuilder::new("cmd");
            c.args(["/C", "set /p X=&& echo got=%X%"]);
            c
        } else {
            let mut c = CommandBuilder::new("sh");
            c.args(["-c", "read X; echo got=$X"]);
            c
        };
        cmd.cwd(std::env::temp_dir());
        let session = spawn(cmd).expect("spawn read");
        let mut writer = session.writer().expect("take writer");
        // Writing should not error.
        assert!(writeln!(writer, "test_value").is_ok());
    }
}
