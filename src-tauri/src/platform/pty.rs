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
        writeln!(writer, "world").expect("write line");
        drop(writer);
        let reader = session.reader().expect("clone reader");
        let mut br = BufReader::new(reader);
        let mut out = String::new();
        for _ in 0..10 {
            let mut line = String::new();
            if br.read_line(&mut line).is_err() {
                break;
            }
            out.push_str(&line);
            if out.contains("got=world") {
                break;
            }
        }
        assert!(out.contains("got=world"), "expected got=world, saw {out:?}");
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
}
