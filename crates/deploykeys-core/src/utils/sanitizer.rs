use regex::Regex;
use std::sync::OnceLock;

static TOKEN_PATTERN: OnceLock<Regex> = OnceLock::new();
static AUTH_PATTERN: OnceLock<Regex> = OnceLock::new();
static PASSWORD_PATTERN: OnceLock<Regex> = OnceLock::new();
static JSON_TOKEN_PATTERN: OnceLock<Regex> = OnceLock::new();

/// Replace credential material in `text` with `****` before logging.
pub fn sanitize_log(text: &str) -> String {
    let token_re = TOKEN_PATTERN.get_or_init(|| {
        // Covers user (ghu), OAuth app (gho), server (ghs), refresh (ghr),
        // classic PAT (ghp) and fine-grained PAT (github_pat) prefixes.
        Regex::new(r"(ghu|gho|ghs|ghr|ghp|github_pat)_[A-Za-z0-9_]+").expect("valid regex")
    });

    let auth_re = AUTH_PATTERN.get_or_init(|| {
        Regex::new(r"(?i)Authorization:\s*(Bearer|token|Basic)\s+[A-Za-z0-9._=/+-]+")
            .expect("valid regex")
    });

    let password_re = PASSWORD_PATTERN
        .get_or_init(|| Regex::new(r"(?i)password\s*=\s*[^\s]+").expect("valid regex"));

    let json_token_re = JSON_TOKEN_PATTERN.get_or_init(|| {
        Regex::new(r#"(?i)"(access_token|refresh_token|password|secret)"\s*:\s*"[^"]*""#)
            .expect("valid regex")
    });

    let result = token_re.replace_all(text, "${1}_****");
    let result = auth_re.replace_all(&result, "Authorization: ${1} ****");
    let result = password_re.replace_all(&result, "password=****");
    let result = json_token_re.replace_all(&result, r#""${1}":"****""#);

    result.into_owned()
}

/// Truncate `text` to at most `max_chars` characters for log/error output.
pub fn truncate_for_log(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_chars).collect();
    format!("{}… [truncated]", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_tokens() {
        let input = "Token: ghu_1234567890abcdefghij1234567890";
        let output = sanitize_log(input);
        assert!(
            output.contains("ghu_****"),
            "Expected ghu_****, got: {}",
            output
        );
        assert!(!output.contains("1234567890abcdefghij"));
    }

    #[test]
    fn test_sanitize_oauth_and_server_tokens() {
        let output = sanitize_log("gho_abc123 and ghs_def456");
        assert!(output.contains("gho_****"));
        assert!(output.contains("ghs_****"));
    }

    #[test]
    fn test_sanitize_fine_grained_pat_with_underscores() {
        let output = sanitize_log("github_pat_11ABC_defGHI789");
        assert_eq!(output, "github_pat_****");
    }

    #[test]
    fn test_sanitize_auth_header() {
        let input = "Authorization: Bearer abc123token";
        let output = sanitize_log(input);
        assert_eq!(output, "Authorization: Bearer ****");
    }

    #[test]
    fn test_sanitize_token_scheme_auth_header() {
        let output = sanitize_log("authorization: token abc123");
        assert_eq!(output, "Authorization: token ****");
    }

    #[test]
    fn test_sanitize_password() {
        let input = "password=secret123";
        let output = sanitize_log(input);
        assert_eq!(output, "password=****");
    }

    #[test]
    fn test_sanitize_json_access_token() {
        let input = r#"{"access_token":"super-secret","token_type":"bearer"}"#;
        let output = sanitize_log(input);
        assert!(output.contains(r#""access_token":"****""#));
        assert!(!output.contains("super-secret"));
    }

    #[test]
    fn test_truncate_for_log_short_text_unchanged() {
        assert_eq!(truncate_for_log("short", 10), "short");
    }

    #[test]
    fn test_truncate_for_log_long_text_truncated() {
        let output = truncate_for_log(&"x".repeat(500), 100);
        assert!(output.starts_with(&"x".repeat(100)));
        assert!(output.ends_with("[truncated]"));
        assert!(output.chars().count() < 130);
    }

    #[test]
    fn test_truncate_for_log_is_char_safe() {
        let output = truncate_for_log("密钥密钥密钥", 2);
        assert!(output.starts_with("密钥"));
    }
}
