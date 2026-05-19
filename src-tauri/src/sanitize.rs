//! Sanitises text before it leaves the local machine for Lark Bitable.
//! Two-layer defence: the frontend mirror (`src/lib/sanitize.ts`, Task 14)
//! also runs the same redactions before persisting to `messages.jsonl`, so
//! anything reaching this function should already be clean — but we never
//! trust that.

use std::sync::LazyLock;

use regex::Regex;

// Compile-time-constant regex patterns. `.unwrap()` here is the documented
// exception to the no-unwrap rule: these literals are verified by the unit
// tests below; a parse failure would be a developer bug caught at startup,
// not a runtime input error.
static API_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"sk-[A-Za-z0-9_\-]{20,}").unwrap());
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Bearer\s+[A-Za-z0-9._\-]+").unwrap());
static JWT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"eyJ[A-Za-z0-9._\-]{20,}").unwrap());
// `[^\[\s]\S*` ensures we don't re-match an already-redacted `[REDACTED…]`
// marker left behind by an earlier rule (API_KEY / BEARER / JWT all run
// first and may have rewritten the value into a `[…]` bracket form).
static NAMED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(api[_-]?key|secret|password|token)\s*[:=]\s*[^\[\s]\S*").unwrap()
});

/// Redacts common credential patterns, then truncates the result to
/// `max_chars` characters (Unicode-safe), appending `…` when truncated.
pub fn sanitize_message_preview(input: &str, max_chars: usize) -> String {
    let mut s = API_KEY
        .replace_all(input, "[REDACTED-API-KEY]")
        .into_owned();
    s = BEARER.replace_all(&s, "Bearer [REDACTED]").into_owned();
    s = JWT.replace_all(&s, "[REDACTED-JWT]").into_owned();
    s = NAMED
        .replace_all(&s, |caps: &regex::Captures| {
            format!("{}: [REDACTED]", &caps[1])
        })
        .into_owned();
    truncate_chars(&s, max_chars)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        let mut out = head;
        out.push('…');
        out
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_openai_style_api_key() {
        let s = "Token: sk-proj-abcdefghijklmnopqrstuvwx";
        assert_eq!(
            sanitize_message_preview(s, 200),
            "Token: [REDACTED-API-KEY]"
        );
    }

    #[test]
    fn redacts_bearer_token() {
        let s = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let out = sanitize_message_preview(s, 200);
        assert!(out.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_named_credentials_case_insensitive() {
        let s = "api_key: secret123 and password = topsecret";
        let out = sanitize_message_preview(s, 200);
        assert!(!out.contains("secret123"));
        assert!(!out.contains("topsecret"));
    }

    #[test]
    fn truncates_to_max_chars_with_ellipsis() {
        let s = "a".repeat(500);
        let out = sanitize_message_preview(&s, 200);
        // 200 ascii chars + 1 ellipsis char (3 bytes UTF-8 for '…').
        // The plan's original `out.len() == 201` only held for ASCII
        // ellipsis; we count chars instead — same intent, correctly.
        assert_eq!(out.chars().count(), 201);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn short_strings_pass_through_untruncated() {
        let s = "Hello world";
        assert_eq!(sanitize_message_preview(s, 200), "Hello world");
    }

    #[test]
    fn redacts_multiple_credentials_in_one_message() {
        // Verifies the NAMED `[^\[\s]\S*` carve-out: after API_KEY rewrites the
        // sk-... value to [REDACTED-API-KEY], NAMED must NOT match that marker
        // (its value class excludes leading `[`), so the more specific marker is
        // preserved AND the original separator `=` is left intact. Also exercises
        // BEARER + API_KEY in the same input.
        let s = "call with Bearer abc.def and api_key=sk-proj-aaaaaaaaaaaaaaaaaaaa";
        let out = sanitize_message_preview(s, 200);
        assert_eq!(
            out,
            "call with Bearer [REDACTED] and api_key=[REDACTED-API-KEY]"
        );
    }

    #[test]
    fn truncation_runs_after_redaction() {
        // A long Bearer prefix is fully redacted before length is measured, so
        // the trailing " tail" should still appear in the output. If truncation
        // ran first, "tail" would be lost.
        let mut s = String::from("Bearer ");
        s.push_str(&"a".repeat(500));
        s.push_str(" tail");
        let out = sanitize_message_preview(&s, 50);
        assert!(out.contains("Bearer [REDACTED]"));
        assert!(out.contains("tail"), "expected 'tail' to survive: {out:?}");
    }

    #[test]
    fn empty_input_returns_empty_string() {
        assert_eq!(sanitize_message_preview("", 200), "");
    }

    #[test]
    fn named_keyword_only_matches_at_word_boundary() {
        // "secret" is part of "mysecret" / "username=secret123" — the \b in NAMED
        // must prevent the keyword from being recognised mid-word. The value
        // "secret123" sits on the RHS of a `=` but the LHS keyword is "username",
        // not a credential keyword, so nothing should be redacted.
        let s = "Set username=secret123 and mysecret: foo";
        let out = sanitize_message_preview(s, 200);
        assert_eq!(out, "Set username=secret123 and mysecret: foo");
    }

    #[test]
    fn named_redaction_preserves_keyword_case_and_format() {
        // Stronger positive assertion than `redacts_named_credentials_case_insensitive`
        // — locks in the exact output shape, which would have failed under the
        // plan's original `\S+` regex (would have produced double redactions).
        let s = "api_key: secret123 and password = topsecret";
        let out = sanitize_message_preview(s, 200);
        assert_eq!(out, "api_key: [REDACTED] and password: [REDACTED]");
    }
}
