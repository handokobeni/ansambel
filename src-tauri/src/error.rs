use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Git operation failed: {0}")]
    Git(String),

    #[error("External command failed: {cmd}: {msg}")]
    Command { cmd: String, msg: String },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),

    #[error("Other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts_via_question_mark() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "file.txt");
        let app: AppError = inner.into();
        assert!(app.to_string().contains("I/O error"));
        assert!(app.to_string().contains("file.txt"));
    }

    #[test]
    fn serde_error_converts() {
        let bad: std::result::Result<serde_json::Value, _> = serde_json::from_str("{invalid");
        let err = bad.unwrap_err();
        let app: AppError = err.into();
        assert!(matches!(app, AppError::Serde(_)));
        assert!(app.to_string().contains("Serialization"));
    }

    #[test]
    fn command_error_formats_cmd_and_msg() {
        let e = AppError::Command {
            cmd: "git".into(),
            msg: "not found".into(),
        };
        assert_eq!(e.to_string(), "External command failed: git: not found");
    }

    #[test]
    fn not_found_contains_identifier() {
        let e = AppError::NotFound("repo_abc".into());
        assert_eq!(e.to_string(), "Not found: repo_abc");
    }

    #[test]
    fn path_not_found_includes_path() {
        let e = AppError::PathNotFound(PathBuf::from("/tmp/x"));
        assert!(e.to_string().contains("/tmp/x"));
    }

    #[test]
    fn app_error_converts_to_string_for_tauri_commands() {
        let e = AppError::Other("oops".into());
        let s: String = e.into();
        assert_eq!(s, "Other: oops");
    }
}
