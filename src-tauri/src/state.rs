use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct AppState {
    pub repos: HashMap<String, RepoInfo>,
    pub workspaces: HashMap<String, WorkspaceInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
}

pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_default_is_empty() {
        let s = AppState::default();
        assert!(s.repos.is_empty());
        assert!(s.workspaces.is_empty());
    }

    #[test]
    fn app_version_matches_cargo_pkg_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }
}
