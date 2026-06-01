use serde_json::Value;

pub fn pointer_string<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

pub fn chat_content(value: &Value) -> Option<&str> {
    pointer_string(value, "/choices/0/message/content")
}

pub fn request_id_from_headers(
    headers: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    headers
        .get("x-request-id")
        .or_else(|| headers.get("openai-request-id"))
        .cloned()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{chat_content, pointer_string, request_id_from_headers};

    #[test]
    fn extracts_chat_content() {
        let value = json!({
            "choices": [
                {"message": {"content": "pong"}}
            ]
        });

        assert_eq!(chat_content(&value), Some("pong"));
        assert_eq!(
            pointer_string(&value, "/choices/0/message/content"),
            Some("pong")
        );
    }

    #[test]
    fn request_id_prefers_x_request_id() {
        let mut headers = BTreeMap::new();
        headers.insert("openai-request-id".to_string(), "openai-req".to_string());
        headers.insert("x-request-id".to_string(), "x-req".to_string());

        assert_eq!(request_id_from_headers(&headers).as_deref(), Some("x-req"));
    }
}
