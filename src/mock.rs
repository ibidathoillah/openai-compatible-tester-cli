use anyhow::Context;
use clap::ValueEnum;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MockMode {
    Compatible,
    Partial,
    Malformed,
    SlowStream,
}

pub async fn run_mock_server(port: u16, mode: MockMode) -> anyhow::Result<i32> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("failed to bind mock server on port {port}"))?;
    println!("octest mock server listening on http://127.0.0.1:{port}/v1 ({mode:?})");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, mode).await {
                eprintln!("mock server connection error: {err:#}");
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream, mode: MockMode) -> anyhow::Result<()> {
    let request = read_request(&mut stream).await?;
    let response = route(&request, mode).await;
    stream.write_all(&response).await?;
    stream.shutdown().await?;
    Ok(())
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    body: String,
}

async fn read_request(stream: &mut TcpStream) -> anyhow::Result<Request> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            anyhow::bail!("connection closed before headers");
        }
        buffer.extend_from_slice(&temp[..n]);
        if let Some(pos) = find_header_end(&buffer) {
            header_end = pos;
            break;
        }
        if buffer.len() > 64 * 1024 {
            anyhow::bail!("request headers too large");
        }
    }

    let headers_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = headers_text.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let content_length = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .next()
        .unwrap_or(0);

    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..n]);
    }

    let body =
        String::from_utf8_lossy(&buffer[body_start..buffer.len().min(body_start + content_length)])
            .to_string();

    Ok(Request { method, path, body })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn route(request: &Request, mode: MockMode) -> Vec<u8> {
    if mode == MockMode::Malformed {
        return response_json(200, "application/json", b"{not valid json".to_vec());
    }

    let path = request.path.trim_start_matches("/v1/");
    match (request.method.as_str(), path) {
        ("GET", "models") => response(
            200,
            json!({
                "object": "list",
                "data": [
                    {"id": "mock-chat", "object": "model"},
                    {"id": "mock-embedding", "object": "model"}
                ]
            }),
        ),
        ("GET", model_path) if model_path.starts_with("models/") => {
            let id = model_path.trim_start_matches("models/");
            response(200, json!({"id": id, "object": "model"}))
        }
        ("POST", "chat/completions") => chat_completion(request, mode).await,
        ("POST", "embeddings") => embeddings(request),
        _ => response(404, error("not_found", "endpoint not found")),
    }
}

async fn chat_completion(request: &Request, mode: MockMode) -> Vec<u8> {
    let body: Value = match serde_json::from_str(&request.body) {
        Ok(value) => value,
        Err(_) => return response(400, error("invalid_request_error", "invalid JSON")),
    };

    if body.get("model").and_then(Value::as_str) == Some("definitely-invalid-model-for-octest") {
        return response(404, error("invalid_request_error", "model not found"));
    }

    if body.get("stream").and_then(Value::as_bool) == Some(true) {
        return stream_response(&body, mode).await;
    }

    if body.get("tools").is_some() {
        if mode == MockMode::Partial {
            return response(
                400,
                error("invalid_request_error", "unsupported parameter: tools"),
            );
        }
        return response(
            200,
            json!({
                "id": "chatcmpl-mock-tool",
                "object": "chat.completion",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_mock_weather",
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"city\":\"Jakarta\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 1, "total_tokens": 11}
            }),
        );
    }

    if let Some(messages) = body.get("messages").and_then(Value::as_array) {
        if messages
            .iter()
            .any(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
        {
            return chat_text("The weather in Jakarta is sunny and 30C.");
        }
    }

    if body
        .pointer("/response_format/type")
        .and_then(Value::as_str)
        == Some("json_schema")
    {
        if mode == MockMode::Partial {
            return chat_text("{\"name\":\"Budi\",\"age\":\"20\"}");
        }
        return chat_text("{\"name\":\"Budi\",\"age\":20}");
    }

    if body
        .pointer("/response_format/type")
        .and_then(Value::as_str)
        == Some("json_object")
    {
        return chat_text("{\"name\":\"Budi\",\"age\":20}");
    }

    let prompt = body
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.last())
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if prompt.contains("What is my code?") {
        return chat_text("ABC123");
    }
    if prompt.contains("say pong") {
        return chat_text("PONG");
    }
    chat_text("pong")
}

fn chat_text(content: &str) -> Vec<u8> {
    response(
        200,
        json!({
            "id": "chatcmpl-mock",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6}
        }),
    )
}

async fn stream_response(body: &Value, mode: MockMode) -> Vec<u8> {
    let include_usage = body
        .pointer("/stream_options/include_usage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut payload = Vec::new();
    payload.extend_from_slice(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n");
    if mode == MockMode::SlowStream {
        sleep(Duration::from_millis(150)).await;
    }
    payload.extend_from_slice(br#"data: {"id":"chatcmpl-stream","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"pong"},"finish_reason":null}]}"#);
    payload.extend_from_slice(b"\n\n");
    if include_usage {
        payload.extend_from_slice(br#"data: {"id":"chatcmpl-stream","object":"chat.completion.chunk","choices":[],"usage":{"prompt_tokens":4,"completion_tokens":1,"total_tokens":5}}"#);
        payload.extend_from_slice(b"\n\n");
    }
    payload.extend_from_slice(br#"data: {"id":"chatcmpl-stream","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#);
    payload.extend_from_slice(b"\n\n");
    payload.extend_from_slice(b"data: [DONE]\n\n");
    payload
}

fn embeddings(request: &Request) -> Vec<u8> {
    let body: Value = match serde_json::from_str(&request.body) {
        Ok(value) => value,
        Err(_) => return response(400, error("invalid_request_error", "invalid JSON")),
    };
    let count = if body.get("input").and_then(Value::as_array).is_some() {
        body["input"].as_array().map(Vec::len).unwrap_or(0)
    } else {
        1
    };
    let data = (0..count)
        .map(|index| json!({"object": "embedding", "index": index, "embedding": [0.1, 0.2, 0.3]}))
        .collect::<Vec<_>>();
    response(
        200,
        json!({
            "object": "list",
            "data": data,
            "model": body.get("model").cloned().unwrap_or_else(|| json!("mock-embedding")),
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        }),
    )
}

fn response(status: u16, value: Value) -> Vec<u8> {
    response_json(
        status,
        "application/json",
        serde_json::to_vec(&value).unwrap(),
    )
}

fn error(kind: &str, message: &str) -> Value {
    json!({
        "error": {
            "message": message,
            "type": kind,
            "code": kind
        }
    })
}

fn response_json(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}
