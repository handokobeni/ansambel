//! Atomic persistence for the team-activity Bitable config (Phase 3a-3).
//! The file is absent by default (publisher disabled). When present and
//! `app_token` is non-empty, the publisher subscribes to the workspace
//! event broadcast and starts upserting rows.

use crate::error::Result;
use crate::persistence::atomic::write_atomic;
use crate::state::TeamActivityConfig;
use std::path::{Path, PathBuf};

fn cfg_path(data_dir: &Path) -> PathBuf {
    data_dir.join("team_activity_config.json")
}

pub fn save_team_activity_config(data_dir: &Path, cfg: &TeamActivityConfig) -> Result<()> {
    write_atomic(&cfg_path(data_dir), cfg)
}

/// Returns `Some(cfg)` only when the file exists AND `app_token` is
/// non-empty. Empty token = user disabled publishing; treat as if no
/// config were present so the spawn site can use a single `if let Some`
/// gate.
pub fn load_team_activity_config(data_dir: &Path) -> Result<Option<TeamActivityConfig>> {
    let path = cfg_path(data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let cfg: TeamActivityConfig = serde_json::from_slice(&bytes)?;
    if cfg.app_token.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(cfg))
}

pub fn delete_team_activity_config(data_dir: &Path) -> Result<()> {
    let path = cfg_path(data_dir);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TeamActivityConfig;

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: "bascntest".into(),
            table_id: "tblTeamActivity".into(),
            machine_label: "handoko@laptop-1".into(),
        };
        save_team_activity_config(tmp.path(), &cfg).unwrap();
        let loaded = load_team_activity_config(tmp.path()).unwrap();
        assert_eq!(loaded, Some(cfg));
    }

    #[test]
    fn load_returns_none_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_team_activity_config(tmp.path()).unwrap(), None);
    }

    #[test]
    fn load_returns_none_when_app_token_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: String::new(),
            table_id: "tbl".into(),
            machine_label: "m".into(),
        };
        save_team_activity_config(tmp.path(), &cfg).unwrap();
        // Empty app_token = "disabled" = treat as no config
        assert_eq!(load_team_activity_config(tmp.path()).unwrap(), None);
    }

    #[test]
    fn delete_is_idempotent_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        // Should not error even though the file never existed
        delete_team_activity_config(tmp.path()).unwrap();
    }

    #[test]
    fn delete_removes_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TeamActivityConfig {
            app_token: "x".into(),
            table_id: "y".into(),
            machine_label: "z".into(),
        };
        save_team_activity_config(tmp.path(), &cfg).unwrap();
        delete_team_activity_config(tmp.path()).unwrap();
        assert_eq!(load_team_activity_config(tmp.path()).unwrap(), None);
    }
}
