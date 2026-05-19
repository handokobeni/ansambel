pub mod binary;
pub mod lark_client;
pub mod paths;
pub mod pty;
pub mod repo_identity;

#[cfg(test)]
mod dep_tests {
    #[test]
    fn portable_pty_is_resolvable() {
        // Compile-only: ensure the crate is in scope.
        let _ = std::any::type_name::<portable_pty::PtySize>();
    }
}
