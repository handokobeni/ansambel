use crate::error::{AppError, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};

const DEFAULT_ROWS: u16 = 30;
const DEFAULT_COLS: u16 = 120;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    pid: u32,
}

pub fn spawn(cmd: CommandBuilder) -> Result<PtySession> {
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
    Ok(PtySession {
        master: pair.master,
        child,
        pid,
    })
}

impl PtySession {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn reader(&self) -> Result<Box<dyn Read + Send>> {
        self.master
            .try_clone_reader()
            .map_err(|e| AppError::Command {
                cmd: "pty.reader".into(),
                msg: e.to_string(),
            })
    }

    pub fn writer(&self) -> Result<Box<dyn Write + Send>> {
        self.master.take_writer().map_err(|e| AppError::Command {
            cmd: "pty.writer".into(),
            msg: e.to_string(),
        })
    }

    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().map_err(|e| AppError::Command {
            cmd: "kill".into(),
            msg: e.to_string(),
        })?;
        Ok(())
    }

    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>> {
        self.child.try_wait().map_err(|e| AppError::Command {
            cmd: "try_wait".into(),
            msg: e.to_string(),
        })
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master
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
        // On Windows: emit "ready" before set /p so the reader thread can
        // signal us exactly when the child is waiting for stdin.  Dropping
        // the writer before the child reads the buffered data causes ConPTY
        // to deliver EOF before the payload, so we keep the writer alive
        // until "got=world" is confirmed in the output.
        #[cfg(windows)]
        let cmd = {
            let mut c = CommandBuilder::new("cmd");
            c.args(["/C", "echo ready && set /p X= && echo got=%X%"]);
            c.cwd(std::env::temp_dir());
            c
        };
        #[cfg(not(windows))]
        let cmd = {
            let mut c = CommandBuilder::new("sh");
            c.args(["-c", "read X; echo got=$X"]);
            c.cwd(std::env::temp_dir());
            c
        };
        let session = spawn(cmd).expect("spawn");
        let reader = session.reader().expect("reader");
        let mut writer = session.writer().expect("writer");

        #[cfg(windows)]
        {
            // Two channels: one to signal "ready", one to return the full output.
            let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
            let (out_tx, out_rx) = std::sync::mpsc::channel::<String>();
            std::thread::spawn(move || {
                let mut br = BufReader::new(reader);
                let mut out = String::new();
                let mut signaled = false;
                for _ in 0..50 {
                    let mut line = String::new();
                    match br.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    out.push_str(&line);
                    if !signaled && out.contains("ready") {
                        let _ = ready_tx.send(());
                        signaled = true;
                    }
                    if out.contains("got=world") {
                        break;
                    }
                }
                let _ = out_tx.send(out);
            });
            ready_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("child did not print 'ready' within 10 s");
            // set /p is now waiting for input; write CRLF (Windows console line end).
            writer.write_all(b"world\r\n").expect("write");
            // Flush immediately: the underlying writer may be buffered, and if we
            // drop it before flushing, the pipe closes (EOF) before set /p ever
            // sees the payload — causing it to hang waiting for "real" input.
            writer.flush().expect("flush");
            // Keep writer alive until output is confirmed so ConPTY does not
            // deliver EOF to the child before it has processed the line.
            let out = out_rx
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_default();
            drop(writer);
            assert!(
                out.contains("got=world"),
                "expected got=world in PTY output, got: {out:?}"
            );
        }
        #[cfg(not(windows))]
        {
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
