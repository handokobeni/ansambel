pub(crate) fn get_app_version_impl() -> &'static str {
    crate::state::app_version()
}

#[tauri::command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(get_app_version_impl().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_app_version_command_returns_version() {
        let v = get_app_version_impl();
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }
}
