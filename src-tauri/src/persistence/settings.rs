use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::app_settings_file;
use crate::state::AppSettings;
use std::path::Path;

pub fn load_settings(data_dir: &Path) -> Result<AppSettings> {
    load_or_default(&app_settings_file(data_dir))
}

pub fn save_settings(data_dir: &Path, settings: &AppSettings) -> Result<()> {
    write_atomic(&app_settings_file(data_dir), settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_settings_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let s = crate::state::AppSettings {
            theme: "cool-light".into(),
            onboarding_completed: true,
            ..Default::default()
        };
        save_settings(tmp.path(), &s).unwrap();

        let loaded = load_settings(tmp.path()).unwrap();
        assert_eq!(loaded.theme, "cool-light");
        assert!(loaded.onboarding_completed);
    }

    #[test]
    fn load_settings_missing_file_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_settings(tmp.path()).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.theme, "warm-dark");
    }

    #[test]
    fn save_settings_serializes_all_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let s = crate::state::AppSettings {
            selected_repo_id: Some("repo_abc".into()),
            recent_repos: vec!["repo_abc".into()],
            ..Default::default()
        };
        save_settings(tmp.path(), &s).unwrap();

        let content =
            std::fs::read_to_string(crate::platform::paths::app_settings_file(tmp.path())).unwrap();
        assert!(content.contains("\"selected_repo_id\""));
        assert!(content.contains("repo_abc"));
    }
}
