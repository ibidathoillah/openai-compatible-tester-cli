const REDACTION: &str = "***REDACTED***";

pub fn redact_secret(input: &str, secret: Option<&str>, enabled: bool) -> String {
    if !enabled {
        return input.to_string();
    }

    let mut output = input.to_string();
    if let Some(secret) = secret {
        if !secret.is_empty() {
            output = output.replace(secret, REDACTION);
        }
    }

    redact_authorization_value(&output)
}

fn redact_authorization_value(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(idx) = lower.find("authorization: bearer ") {
            let prefix = &line[..idx + "authorization: bearer ".len()];
            out.push_str(prefix);
            out.push_str(REDACTION);
        } else if let Some(idx) = lower.find("\"authorization\"") {
            if let Some(bearer_idx) = lower[idx..].find("bearer ") {
                let token_start = idx + bearer_idx + "bearer ".len();
                let token_end = line[token_start..]
                    .find('"')
                    .map(|pos| token_start + pos)
                    .unwrap_or(line.len());
                out.push_str(&line[..token_start]);
                out.push_str(REDACTION);
                out.push_str(&line[token_end..]);
            } else {
                out.push_str(line);
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }

    if !input.ends_with('\n') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::redact_secret;

    #[test]
    fn redacts_direct_secret() {
        let text = "key sk-test-123";
        assert_eq!(
            redact_secret(text, Some("sk-test-123"), true),
            "key ***REDACTED***"
        );
    }

    #[test]
    fn redacts_bearer_header() {
        let text = "Authorization: Bearer sk-test-123";
        assert_eq!(
            redact_secret(text, None, true),
            "Authorization: Bearer ***REDACTED***"
        );
    }

    #[test]
    fn leaves_secret_when_redaction_disabled() {
        let text = "key sk-test-123";
        assert_eq!(
            redact_secret(text, Some("sk-test-123"), false),
            "key sk-test-123"
        );
    }

    #[test]
    fn redacts_multiline_authorization_header_case_insensitively() {
        let text = "content-type: application/json\nauthorization: bearer sk-test-123\nx: y";
        assert_eq!(
            redact_secret(text, None, true),
            "content-type: application/json\nauthorization: bearer ***REDACTED***\nx: y"
        );
    }

    #[test]
    fn redacts_json_authorization_line() {
        let text = r#"{"authorization":"Bearer sk-test-123"}"#;
        assert_eq!(
            redact_secret(text, None, true),
            r#"{"authorization":"Bearer ***REDACTED***"}"#
        );
    }

    #[test]
    fn leaves_json_authorization_without_bearer_unchanged() {
        let text = r#"{"authorization":"Basic abc"}"#;
        assert_eq!(redact_secret(text, None, true), text);
    }

    #[test]
    fn preserves_trailing_newline() {
        let text = "Authorization: Bearer sk-test-123\n";
        assert_eq!(
            redact_secret(text, None, true),
            "Authorization: Bearer ***REDACTED***\n"
        );
    }
}
