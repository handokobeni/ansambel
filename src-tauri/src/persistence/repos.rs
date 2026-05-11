use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::repos_file;
use crate::state::RepoInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct ReposFile {
    schema_version: u32,
    repos: HashMap<String, RepoInfo>,
}

pub fn load_repos(data_dir: &Path) -> Result<HashMap<String, RepoInfo>> {
    let file: ReposFile = load_or_default(&repos_file(data_dir))?;
    Ok(file.repos)
}

pub fn save_repos(data_dir: &Path, repos: &HashMap<String, RepoInfo>) -> Result<()> {
    let file = ReposFile {
        schema_version: 1,
        repos: repos.clone(),
    };
    write_atomic(&repos_file(data_dir), &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_repo(id: &str) -> RepoInfo {
        RepoInfo {
            id: id.into(),
            name: "test-repo".into(),
            path: PathBuf::from("/tmp/test-repo"),
            gh_profile: None,
            default_branch: "main".into(),
            created_at: 1_000_000,
            updated_at: 1_000_001,
            scripts: Vec::new(),
        }
    }

    #[test]
    fn save_and_load_repos_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = HashMap::new();
        map.insert("repo_abc".into(), make_repo("repo_abc"));
        save_repos(tmp.path(), &map).unwrap();

        let loaded = load_repos(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["repo_abc"].name, "test-repo");
    }

    #[test]
    fn load_repos_missing_file_returns_empty_map() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_repos(tmp.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn legacy_repos_json_without_scripts_field_loads_with_empty_vec() {
        // Phase 2b regression guard: pre-Phase-2b repos.json files
        // omit the `scripts` field entirely. `#[serde(default)]` on
        // RepoInfo.scripts must let them load with an empty vec.
        let tmp = tempfile::tempdir().unwrap();
        let path = crate::platform::paths::repos_file(tmp.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy_json = r#"{
  "schema_version": 1,
  "repos": {
    "repo_legacy": {
      "id": "repo_legacy",
      "name": "legacy",
      "path": "/tmp/legacy",
      "gh_profile": null,
      "default_branch": "main",
      "created_at": 1,
      "updated_at": 2
    }
  }
}"#;
        std::fs::write(&path, legacy_json).unwrap();
        let loaded = load_repos(tmp.path()).unwrap();
        assert_eq!(
            loaded["repo_legacy"].scripts,
            Vec::<crate::state::RepoScript>::new()
        );
    }

    #[test]
    fn save_and_load_round_trips_scripts_field() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = HashMap::new();
        let mut repo = make_repo("repo_with_scripts");
        repo.scripts = vec![
            crate::state::RepoScript {
                id: "sc_1".into(),
                name: "Run tests".into(),
                command: "bun test".into(),
            },
            crate::state::RepoScript {
                id: "sc_2".into(),
                name: "Lint".into(),
                command: "bun run lint".into(),
            },
        ];
        map.insert("repo_with_scripts".into(), repo);
        save_repos(tmp.path(), &map).unwrap();

        let loaded = load_repos(tmp.path()).unwrap();
        let scripts = &loaded["repo_with_scripts"].scripts;
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].name, "Run tests");
        assert_eq!(scripts[1].command, "bun run lint");
    }

    #[test]
    fn save_repos_writes_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let map: HashMap<String, RepoInfo> = HashMap::new();
        save_repos(tmp.path(), &map).unwrap();

        let content =
            std::fs::read_to_string(crate::platform::paths::repos_file(tmp.path())).unwrap();
        assert!(content.contains("\"schema_version\""));
        assert!(content.contains("\"repos\""));
    }
}
