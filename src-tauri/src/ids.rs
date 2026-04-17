use nanoid::nanoid;

const ALPHABET: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i',
    'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

fn id_body() -> String {
    nanoid!(6, ALPHABET)
}

pub fn repo_id() -> String {
    format!("repo_{}", id_body())
}
pub fn workspace_id() -> String {
    format!("ws_{}", id_body())
}
pub fn message_id() -> String {
    format!("msg_{}", id_body())
}
pub fn todo_id() -> String {
    format!("td_{}", id_body())
}
pub fn script_id() -> String {
    format!("sc_{}", id_body())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn repo_id_has_prefix_and_length() {
        let id = repo_id();
        assert!(id.starts_with("repo_"));
        assert_eq!(id.len(), "repo_".len() + 6);
    }

    #[test]
    fn workspace_id_has_prefix_and_length() {
        let id = workspace_id();
        assert!(id.starts_with("ws_"));
        assert_eq!(id.len(), "ws_".len() + 6);
    }

    #[test]
    fn message_id_has_prefix() {
        assert!(message_id().starts_with("msg_"));
    }

    #[test]
    fn todo_id_has_prefix() {
        assert!(todo_id().starts_with("td_"));
    }

    #[test]
    fn script_id_has_prefix() {
        assert!(script_id().starts_with("sc_"));
    }

    #[test]
    fn thousand_ids_have_no_collisions() {
        // 6-char nanoid with 36-char alphabet → ~2.2B possibilities.
        // Birthday-paradox collision probability for N samples ≈ N² / (2 * 36^6):
        //   10_000 samples → ~0.23% flake rate (1 in 440 runs)
        //    1_000 samples → ~0.0023% flake rate (effectively never)
        // Keep the test guarding ID uniqueness without being CI-flaky.
        let set: HashSet<String> = (0..1_000).map(|_| workspace_id()).collect();
        assert_eq!(set.len(), 1_000);
    }

    #[test]
    fn ids_use_only_allowed_alphabet() {
        let id = workspace_id();
        let body = id.strip_prefix("ws_").unwrap();
        for c in body.chars() {
            assert!(
                c.is_ascii_alphanumeric() && c.is_ascii_lowercase() || c.is_ascii_digit(),
                "Unexpected char {:?} in id {}",
                c,
                id
            );
        }
    }
}
