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

    #[tokio::test]
    async fn get_app_version_async_command_returns_ok() {
        let result = get_app_version().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), env!("CARGO_PKG_VERSION"));
    }
}
