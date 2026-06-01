use std::collections::BTreeSet;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::client::{ApiClient, ClientError, HttpResponse};
use crate::config::RunConfig;
use crate::types::{Profile, TestResult, TestStatus};
use crate::util::json::{chat_content, request_id_from_headers};
use crate::util::redact::redact_secret;

#[async_trait]
pub trait TestCase: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn category(&self) -> &str;
    fn weight(&self) -> u32;
    fn profiles(&self) -> Vec<Profile>;
    fn required(&self) -> bool;
    async fn run(&self, config: &RunConfig, client: &ApiClient) -> TestResult;
}

#[derive(Debug, Clone)]
pub struct BuiltinTest {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    weight: u32,
    profiles: &'static [Profile],
    required: bool,
    kind: TestKind,
}

#[derive(Debug, Clone)]
enum TestKind {
    AuthBearerValid,
    ModelsList,
    ModelsRetrieve,
    ChatBasic,
    ChatSystemMessage,
    ChatMultiTurn,
    ChatParameters,
    ChatUsage,
    ChatStream,
    ChatStreamUsage,
    ToolCall,
    ToolResultFlow,
    JsonMode,
    StructuredOutput,
    EmbeddingsSingle,
    EmbeddingsBatch,
    ErrorInvalidModel,
    ErrorInvalidJson,
    ManualChat(String),
}

#[async_trait]
impl TestCase for BuiltinTest {
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &str {
        self.name
    }

    fn category(&self) -> &str {
        self.category
    }

    fn weight(&self) -> u32 {
        self.weight
    }

    fn profiles(&self) -> Vec<Profile> {
        self.profiles.to_vec()
    }

    fn required(&self) -> bool {
        self.required
    }

    async fn run(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        match &self.kind {
            TestKind::AuthBearerValid => self.auth_bearer_valid(config, client).await,
            TestKind::ModelsList => self.models_list(config, client).await,
            TestKind::ModelsRetrieve => self.models_retrieve(config, client).await,
            TestKind::ChatBasic => self.chat_basic(config, client).await,
            TestKind::ChatSystemMessage => self.chat_system_message(config, client).await,
            TestKind::ChatMultiTurn => self.chat_multi_turn(config, client).await,
            TestKind::ChatParameters => self.chat_parameters(config, client).await,
            TestKind::ChatUsage => self.chat_usage(config, client).await,
            TestKind::ChatStream => self.chat_stream(config, client).await,
            TestKind::ChatStreamUsage => self.chat_stream_usage(config, client).await,
            TestKind::ToolCall => self.tool_call(config, client).await,
            TestKind::ToolResultFlow => self.tool_result_flow(config, client).await,
            TestKind::JsonMode => self.json_mode(config, client).await,
            TestKind::StructuredOutput => self.structured_output(config, client).await,
            TestKind::EmbeddingsSingle => self.embeddings_single(config, client).await,
            TestKind::EmbeddingsBatch => self.embeddings_batch(config, client).await,
            TestKind::ErrorInvalidModel => self.error_invalid_model(config, client).await,
            TestKind::ErrorInvalidJson => self.error_invalid_json(config, client).await,
            TestKind::ManualChat(message) => self.manual_chat(config, client, message).await,
        }
    }
}

impl BuiltinTest {
    async fn auth_bearer_valid(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        if config.no_auth {
            return self.result(
                config,
                TestStatus::Passed,
                0,
                None,
                json!({"mode": "no_auth", "message": "Authorization header disabled"}),
            );
        }

        match client.get("models", config.timeouts.request_ms).await {
            Ok(resp) if !matches!(resp.status, 401 | 403) => self.from_response(
                config,
                TestStatus::Passed,
                &resp,
                None,
                json!({"status": resp.status}),
            ),
            Ok(resp) => self.from_response(
                config,
                TestStatus::Failed,
                &resp,
                Some("provider rejected bearer authentication"),
                error_details(&resp),
            ),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn models_list(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        match client.get("models", config.timeouts.request_ms).await {
            Ok(resp) => {
                match parse_json_response(&resp) {
                    Ok(value)
                        if resp.status == 200
                            && value.get("data").and_then(Value::as_array).is_some() =>
                    {
                        let has_ids = value
                            .get("data")
                            .and_then(Value::as_array)
                            .map(|models| models.iter().any(|model| model.get("id").is_some()))
                            .unwrap_or(false);
                        let status = if has_ids {
                            TestStatus::Passed
                        } else {
                            TestStatus::Failed
                        };
                        self.from_response(
                        config,
                        status,
                        &resp,
                        if has_ids { None } else { Some("models list has no model id") },
                        json!({"model_count": value["data"].as_array().map(Vec::len).unwrap_or(0)}),
                    )
                    }
                    Ok(_) => self.from_response(
                        config,
                        TestStatus::Failed,
                        &resp,
                        Some("models response did not match OpenAI-compatible shape"),
                        error_details(&resp),
                    ),
                    Err(err) => self.from_response(
                        config,
                        TestStatus::Failed,
                        &resp,
                        Some(&format!("models response was not valid JSON: {err}")),
                        json!({"status": resp.status}),
                    ),
                }
            }
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn models_retrieve(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let endpoint = format!("models/{}", path_escape(&config.model));
        match client.get(&endpoint, config.timeouts.request_ms).await {
            Ok(resp) if resp.status == 200 => self.from_response(
                config,
                TestStatus::Passed,
                &resp,
                None,
                json!({"status": resp.status}),
            ),
            Ok(resp) => self.from_response(
                config,
                TestStatus::Warning,
                &resp,
                Some("model retrieve is unsupported or target model is absent"),
                error_details(&resp),
            ),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn chat_basic(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Reply with exactly: pong"}],
            "temperature": 0
        });
        self.evaluate_chat_content(config, client, body, |content, value| {
            let finish_reason = value.pointer("/choices/0/finish_reason").is_some();
            if content.to_ascii_lowercase().contains("pong") && finish_reason {
                (TestStatus::Passed, None)
            } else if !content.is_empty() && !finish_reason && !config.features.strict {
                (
                    TestStatus::Warning,
                    Some("finish_reason missing but content is non-empty"),
                )
            } else if !content.is_empty() {
                (
                    TestStatus::Warning,
                    Some("content did not exactly contain expected pong"),
                )
            } else {
                (TestStatus::Failed, Some("assistant content is empty"))
            }
        })
        .await
    }

    async fn chat_system_message(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [
                {"role": "system", "content": "Always answer in uppercase."},
                {"role": "user", "content": "say pong"}
            ],
            "temperature": 0
        });
        self.evaluate_chat_content(config, client, body, |content, _| {
            if content.is_empty() {
                (TestStatus::Failed, Some("assistant content is empty"))
            } else if content == content.to_ascii_uppercase() {
                (TestStatus::Passed, None)
            } else {
                (
                    TestStatus::Warning,
                    Some("system message accepted but uppercase instruction was not followed"),
                )
            }
        })
        .await
    }

    async fn chat_multi_turn(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [
                {"role": "user", "content": "My code is ABC123. Remember it."},
                {"role": "assistant", "content": "I will remember ABC123 for this conversation."},
                {"role": "user", "content": "What is my code?"}
            ],
            "temperature": 0
        });
        self.evaluate_chat_content(config, client, body, |content, _| {
            if content.contains("ABC123") {
                (TestStatus::Passed, None)
            } else if content.is_empty() {
                (TestStatus::Failed, Some("assistant content is empty"))
            } else if config.features.strict {
                (
                    TestStatus::Failed,
                    Some("multi-turn context was not reflected in the answer"),
                )
            } else {
                (
                    TestStatus::Warning,
                    Some("multi-turn context was not reflected in the answer"),
                )
            }
        })
        .await
    }

    async fn chat_parameters(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Reply with one short word."}],
            "temperature": 0,
            "top_p": 1,
            "max_tokens": 16,
            "stop": ["\n\n"],
            "n": 1,
            "presence_penalty": 0,
            "frequency_penalty": 0
        });

        match client.post_json("chat/completions", body, config.timeouts.request_ms).await {
            Ok(resp) if resp.status == 200 => self.from_response(
                config,
                TestStatus::Passed,
                &resp,
                None,
                json!({"accepted": ["temperature", "top_p", "max_tokens", "stop", "n", "presence_penalty", "frequency_penalty"]}),
            ),
            Ok(resp) if error_message(&resp).is_some() => self.from_response(
                config,
                TestStatus::Warning,
                &resp,
                Some("one or more common chat parameters appear unsupported"),
                error_details(&resp),
            ),
            Ok(resp) => self.from_response(
                config,
                TestStatus::Failed,
                &resp,
                Some("parameter test failed with unreadable response"),
                error_details(&resp),
            ),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn chat_usage(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Say hello."}],
            "temperature": 0
        });

        match client
            .post_json("chat/completions", body, config.timeouts.request_ms)
            .await
        {
            Ok(resp) => match parse_json_response(&resp) {
                Ok(value) if resp.status == 200 && value.get("usage").is_some() => self
                    .from_response(
                        config,
                        TestStatus::Passed,
                        &resp,
                        None,
                        json!({"usage_present": true}),
                    ),
                Ok(_) if resp.status == 200 && !config.features.strict => self.from_response(
                    config,
                    TestStatus::Warning,
                    &resp,
                    Some("usage object is missing"),
                    json!({"usage_present": false}),
                ),
                Ok(_) => self.from_response(
                    config,
                    TestStatus::Failed,
                    &resp,
                    Some("usage object is missing"),
                    json!({"usage_present": false}),
                ),
                Err(err) => self.from_response(
                    config,
                    TestStatus::Failed,
                    &resp,
                    Some(&format!("usage response was not valid JSON: {err}")),
                    json!({"status": resp.status}),
                ),
            },
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn chat_stream(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Count from 1 to 5 slowly."}],
            "stream": true
        });

        match client
            .post_stream("chat/completions", body, config.timeouts.stream_ms)
            .await
        {
            Ok(stream) if stream.status != 200 => self.result(
                config,
                TestStatus::Failed,
                stream.total_stream_duration_ms,
                Some("stream endpoint returned non-200 status"),
                json!({"status": stream.status}),
            ),
            Ok(stream) => {
                let content_type = stream
                    .headers
                    .get("content-type")
                    .cloned()
                    .unwrap_or_default();
                let mut status = TestStatus::Passed;
                let mut error = None;
                if stream.delta_content.is_empty() {
                    status = TestStatus::Failed;
                    error = Some("stream did not include delta content");
                } else if !stream.done_received && config.features.strict {
                    status = TestStatus::Failed;
                    error = Some("stream did not include [DONE]");
                } else if !stream.done_received || !content_type.contains("text/event-stream") {
                    status = TestStatus::Warning;
                    error = Some("stream works but is missing ideal SSE behavior");
                }
                self.result(
                    config,
                    status,
                    stream.total_stream_duration_ms,
                    error,
                    json!({
                        "first_token_latency_ms": stream.first_token_latency_ms,
                        "total_stream_duration_ms": stream.total_stream_duration_ms,
                        "chunk_count": stream.chunk_count,
                        "event_count": stream.raw_events.len(),
                        "bytes_received": stream.bytes_received,
                        "done_received": stream.done_received,
                        "content_type": content_type
                    }),
                )
            }
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn chat_stream_usage(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Say hello."}],
            "stream": true,
            "stream_options": {"include_usage": true}
        });

        match client
            .post_stream("chat/completions", body, config.timeouts.stream_ms)
            .await
        {
            Ok(stream) if stream.status == 200 && stream.usage_seen => self.result(
                config,
                TestStatus::Passed,
                stream.total_stream_duration_ms,
                None,
                json!({"usage_seen": true}),
            ),
            Ok(stream) => self.result(
                config,
                if config.features.strict {
                    TestStatus::Failed
                } else {
                    TestStatus::Warning
                },
                stream.total_stream_duration_ms,
                Some("usage was not included in stream"),
                json!({"usage_seen": stream.usage_seen, "status": stream.status}),
            ),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn tool_call(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = tool_call_request(&config.model);
        match client
            .post_json("chat/completions", body, config.timeouts.request_ms)
            .await
        {
            Ok(resp) => self.evaluate_tool_response(config, &resp),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn tool_result_flow(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let first = match client
            .post_json(
                "chat/completions",
                tool_call_request(&config.model),
                config.timeouts.request_ms,
            )
            .await
        {
            Ok(resp) => resp,
            Err(err) => return self.from_client_error(config, err),
        };

        let value = match parse_json_response(&first) {
            Ok(value) => value,
            Err(err) => {
                return self.from_response(
                    config,
                    TestStatus::Failed,
                    &first,
                    Some(&format!("tool call response was not valid JSON: {err}")),
                    json!({"status": first.status}),
                )
            }
        };
        let Some(tool_call) = value.pointer("/choices/0/message/tool_calls/0").cloned() else {
            return self.from_response(
                config,
                TestStatus::Failed,
                &first,
                Some("first request did not produce a tool call"),
                json!({"status": first.status}),
            );
        };
        let tool_call_id = tool_call
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("octest-tool-call-1");

        let body = json!({
            "model": config.model,
            "messages": [
                {"role": "user", "content": "What is the weather in Jakarta? Use the provided tool."},
                {"role": "assistant", "content": null, "tool_calls": [tool_call]},
                {"role": "tool", "tool_call_id": tool_call_id, "name": "get_weather", "content": "{\"city\":\"Jakarta\",\"temperature\":\"30C\",\"condition\":\"sunny\"}"}
            ],
            "temperature": 0
        });

        self.evaluate_chat_content(config, client, body, |content, _| {
            if content.to_ascii_lowercase().contains("jakarta")
                || content.contains("30")
                || content.to_ascii_lowercase().contains("sunny")
            {
                (TestStatus::Passed, None)
            } else if content.is_empty() {
                (
                    TestStatus::Failed,
                    Some("final answer after tool result is empty"),
                )
            } else {
                (
                    TestStatus::Warning,
                    Some("final answer did not clearly use tool result"),
                )
            }
        })
        .await
    }

    async fn json_mode(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Return JSON only with fields name and age for: Budi is 20 years old."}],
            "response_format": {"type": "json_object"}
        });
        self.evaluate_chat_content(config, client, body, |content, _| {
            if content.contains("```") {
                return (
                    TestStatus::Failed,
                    Some("JSON mode response included markdown code fence"),
                );
            }
            match serde_json::from_str::<Value>(content) {
                Ok(value) if value.get("name").is_some() && value.get("age").is_some() => {
                    (TestStatus::Passed, None)
                }
                Ok(_) => (
                    TestStatus::Failed,
                    Some("JSON response is missing name or age"),
                ),
                Err(_) => (
                    TestStatus::Failed,
                    Some("assistant content is not valid JSON"),
                ),
            }
        })
        .await
    }

    async fn structured_output(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": "Extract person from: Budi is 20 years old."}],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "person_schema",
                    "strict": true,
                    "schema": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "age": {"type": "integer"}
                        },
                        "required": ["name", "age"],
                        "additionalProperties": false
                    }
                }
            }
        });
        self.evaluate_chat_content(config, client, body, |content, _| {
            let value = match serde_json::from_str::<Value>(content) {
                Ok(value) => value,
                Err(_) => {
                    return (
                        TestStatus::Failed,
                        Some("structured output content is not valid JSON"),
                    )
                }
            };
            let Some(object) = value.as_object() else {
                return (
                    TestStatus::Failed,
                    Some("structured output is not a JSON object"),
                );
            };
            if object.len() != 2 {
                return (
                    TestStatus::Failed,
                    Some("structured output contains extra properties"),
                );
            }
            if !value.get("name").is_some_and(Value::is_string) {
                return (TestStatus::Failed, Some("name is missing or not a string"));
            }
            if !value.get("age").is_some_and(Value::is_i64) {
                return (TestStatus::Failed, Some("age is missing or not an integer"));
            }
            (TestStatus::Passed, None)
        })
        .await
    }

    async fn embeddings_single(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let model = embedding_model(config);
        if model == "unknown" {
            return self.result(
                config,
                TestStatus::Failed,
                0,
                Some("--embedding-model or config models.embeddings is required"),
                json!({}),
            );
        }
        let body = json!({"model": model, "input": "hello world"});
        match client
            .post_json("embeddings", body, config.timeouts.request_ms)
            .await
        {
            Ok(resp) => self.evaluate_embedding_response(config, &resp, 1),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn embeddings_batch(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let model = embedding_model(config);
        if model == "unknown" {
            return self.result(
                config,
                TestStatus::Failed,
                0,
                Some("--embedding-model or config models.embeddings is required"),
                json!({}),
            );
        }
        let body = json!({"model": model, "input": ["hello", "world"]});
        match client
            .post_json("embeddings", body, config.timeouts.request_ms)
            .await
        {
            Ok(resp) => self.evaluate_embedding_response(config, &resp, 2),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn error_invalid_model(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        let body = json!({
            "model": "definitely-invalid-model-for-octest",
            "messages": [{"role": "user", "content": "hello"}]
        });
        match client
            .post_json("chat/completions", body, config.timeouts.request_ms)
            .await
        {
            Ok(resp)
                if matches!(resp.status, 400 | 404 | 422) && error_message(&resp).is_some() =>
            {
                self.from_response(
                    config,
                    TestStatus::Passed,
                    &resp,
                    None,
                    error_details(&resp),
                )
            }
            Ok(resp) if resp.status == 200 => self.from_response(
                config,
                TestStatus::Warning,
                &resp,
                Some("invalid model returned 200; provider may silently fallback"),
                json!({"status": resp.status}),
            ),
            Ok(resp) => self.from_response(
                config,
                TestStatus::Warning,
                &resp,
                Some("invalid model error was not OpenAI-shaped but was readable"),
                error_details(&resp),
            ),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn error_invalid_json(&self, config: &RunConfig, client: &ApiClient) -> TestResult {
        match client
            .post_raw(
                "chat/completions",
                "{\"model\":".to_string(),
                "application/json",
                config.timeouts.request_ms,
            )
            .await
        {
            Ok(resp) if matches!(resp.status, 400 | 422) => self.from_response(
                config,
                TestStatus::Passed,
                &resp,
                None,
                json!({"status": resp.status}),
            ),
            Ok(resp) if resp.status >= 500 => self.from_response(
                config,
                TestStatus::Failed,
                &resp,
                Some("invalid JSON caused a server error"),
                error_details(&resp),
            ),
            Ok(resp) => self.from_response(
                config,
                TestStatus::Warning,
                &resp,
                Some("invalid JSON did not produce a clear 400-like error"),
                error_details(&resp),
            ),
            Err(err) => self.from_client_error(config, err),
        }
    }

    async fn manual_chat(
        &self,
        config: &RunConfig,
        client: &ApiClient,
        message: &str,
    ) -> TestResult {
        let body = json!({
            "model": config.model,
            "messages": [{"role": "user", "content": message}],
            "temperature": 0
        });
        self.evaluate_chat_content(config, client, body, |content, _| {
            if content.is_empty() {
                (TestStatus::Failed, Some("assistant content is empty"))
            } else {
                (TestStatus::Passed, None)
            }
        })
        .await
    }

    async fn evaluate_chat_content<F>(
        &self,
        config: &RunConfig,
        client: &ApiClient,
        body: Value,
        validate: F,
    ) -> TestResult
    where
        F: FnOnce(&str, &Value) -> (TestStatus, Option<&'static str>) + Send,
    {
        match client
            .post_json("chat/completions", body, config.timeouts.request_ms)
            .await
        {
            Ok(resp) => {
                if resp.status != 200 {
                    return self.from_response(
                        config,
                        TestStatus::Failed,
                        &resp,
                        Some("chat completion returned non-200 status"),
                        error_details(&resp),
                    );
                }
                match parse_json_response(&resp) {
                    Ok(value) => {
                        let content = chat_content(&value).unwrap_or_default();
                        let (status, error) = validate(content, &value);
                        self.from_response(
                            config,
                            status,
                            &resp,
                            error,
                            json!({
                                "content_length": content.len(),
                                "finish_reason": value.pointer("/choices/0/finish_reason").cloned()
                            }),
                        )
                    }
                    Err(err) => self.from_response(
                        config,
                        TestStatus::Failed,
                        &resp,
                        Some(&format!("chat response was not valid JSON: {err}")),
                        json!({"status": resp.status}),
                    ),
                }
            }
            Err(err) => self.from_client_error(config, err),
        }
    }

    fn evaluate_tool_response(&self, config: &RunConfig, resp: &HttpResponse) -> TestResult {
        if resp.status != 200 {
            return self.from_response(
                config,
                TestStatus::Failed,
                resp,
                Some("tool calling request returned non-200 status"),
                error_details(resp),
            );
        }
        let value = match parse_json_response(resp) {
            Ok(value) => value,
            Err(err) => {
                return self.from_response(
                    config,
                    TestStatus::Failed,
                    resp,
                    Some(&format!("tool response was not valid JSON: {err}")),
                    json!({"status": resp.status}),
                )
            }
        };

        let tool = value.pointer("/choices/0/message/tool_calls/0");
        let name = tool
            .and_then(|tool| tool.pointer("/function/name"))
            .and_then(Value::as_str);
        let arguments = tool
            .and_then(|tool| tool.pointer("/function/arguments"))
            .and_then(Value::as_str);
        let args_valid = arguments
            .and_then(|args| serde_json::from_str::<Value>(args).ok())
            .is_some_and(|args| args.get("city").is_some());

        if name == Some("get_weather") && args_valid {
            self.from_response(
                config,
                TestStatus::Passed,
                resp,
                None,
                json!({"tool_name": name, "arguments_valid": args_valid}),
            )
        } else {
            self.from_response(
                config,
                TestStatus::Failed,
                resp,
                Some("forced tool choice did not produce a valid get_weather call"),
                json!({"tool_name": name, "arguments_valid": args_valid}),
            )
        }
    }

    fn evaluate_embedding_response(
        &self,
        config: &RunConfig,
        resp: &HttpResponse,
        expected_count: usize,
    ) -> TestResult {
        if resp.status != 200 {
            return self.from_response(
                config,
                TestStatus::Failed,
                resp,
                Some("embeddings endpoint returned non-200 status"),
                error_details(resp),
            );
        }
        let value = match parse_json_response(resp) {
            Ok(value) => value,
            Err(err) => {
                return self.from_response(
                    config,
                    TestStatus::Failed,
                    resp,
                    Some(&format!("embeddings response was not valid JSON: {err}")),
                    json!({"status": resp.status}),
                )
            }
        };
        let Some(data) = value.get("data").and_then(Value::as_array) else {
            return self.from_response(
                config,
                TestStatus::Failed,
                resp,
                Some("embeddings response is missing data array"),
                json!({}),
            );
        };
        if data.len() != expected_count {
            return self.from_response(
                config,
                TestStatus::Failed,
                resp,
                Some("embeddings response returned unexpected item count"),
                json!({"expected": expected_count, "actual": data.len()}),
            );
        }
        let dims: Vec<usize> = data
            .iter()
            .filter_map(|item| item.get("embedding")?.as_array().map(Vec::len))
            .collect();
        let valid = dims.len() == expected_count
            && dims.iter().all(|dim| *dim > 0)
            && dims.iter().all(|dim| *dim == dims[0])
            && data.iter().all(|item| {
                item.get("embedding")
                    .and_then(Value::as_array)
                    .is_some_and(|embedding| embedding.iter().all(Value::is_number))
            });
        if valid {
            let status = if value.get("usage").is_some() || !config.features.strict {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            self.from_response(
                config,
                status,
                resp,
                if value.get("usage").is_some() {
                    None
                } else {
                    Some("usage object is missing")
                },
                json!({"count": data.len(), "dimension": dims.first().copied().unwrap_or(0)}),
            )
        } else {
            self.from_response(
                config,
                TestStatus::Failed,
                resp,
                Some("embedding vectors are missing, empty, inconsistent, or non-numeric"),
                json!({"dimensions": dims}),
            )
        }
    }

    fn result(
        &self,
        config: &RunConfig,
        status: TestStatus,
        latency_ms: u128,
        error: Option<&str>,
        details: Value,
    ) -> TestResult {
        let score = match status {
            TestStatus::Passed => self.weight,
            TestStatus::Warning => self.weight / 2,
            _ => 0,
        };
        let error =
            error.map(|err| redact_secret(err, config.api_key.as_deref(), config.features.redact));
        TestResult {
            id: self.id.to_string(),
            name: self.name.to_string(),
            category: self.category.to_string(),
            profile: primary_profile(self.profiles).as_str().to_string(),
            status,
            score,
            max_score: self.weight,
            latency_ms,
            request_id: None,
            error,
            details,
        }
    }

    fn from_response(
        &self,
        config: &RunConfig,
        status: TestStatus,
        resp: &HttpResponse,
        error: Option<&str>,
        details: Value,
    ) -> TestResult {
        let mut result = self.result(config, status, resp.latency_ms, error, details);
        result.request_id = request_id_from_headers(&resp.headers);
        result
    }

    fn from_client_error(&self, config: &RunConfig, err: ClientError) -> TestResult {
        let status = if err.is_timeout() {
            TestStatus::Timeout
        } else {
            TestStatus::Error
        };
        self.result(
            config,
            status,
            0,
            Some(&err.to_string()),
            json!({"kind": status.label()}),
        )
    }
}

pub fn registry_for_profiles(profiles: &[Profile], selection: Option<&str>) -> Vec<BuiltinTest> {
    if let Some(selection) = selection {
        return selected_tests(selection);
    }

    let expanded = expand_profiles(profiles);
    all_tests()
        .into_iter()
        .filter(|test| {
            test.profiles
                .iter()
                .any(|profile| expanded.contains(profile))
        })
        .collect()
}

fn selected_tests(selection: &str) -> Vec<BuiltinTest> {
    let mut tests = all_tests();
    if let Some(message) = selection.strip_prefix("manual_chat:") {
        return vec![BuiltinTest {
            id: "chat.manual",
            name: "Manual chat completion",
            category: "chat",
            weight: 1,
            profiles: &[Profile::Core],
            required: true,
            kind: TestKind::ManualChat(message.to_string()),
        }];
    }

    let ids: &[&str] = match selection {
        "quick" => &[
            "models.list",
            "chat.basic",
            "chat.stream",
            "errors.invalid_model",
            "chat.usage",
        ],
        "models" => &["models.list", "models.retrieve"],
        "stream" => &["chat.stream"],
        "tools" => &["chat.tool_call", "chat.tool_result_flow"],
        "schema" => &["chat.json_mode", "chat.structured_output"],
        "embeddings" => &["embeddings.single", "embeddings.batch"],
        _ => &[],
    };
    tests.retain(|test| ids.contains(&test.id));
    tests
}

fn all_tests() -> Vec<BuiltinTest> {
    vec![
        test(
            "auth.bearer_valid",
            "Bearer authentication",
            "auth",
            5,
            &[Profile::Core],
            true,
            TestKind::AuthBearerValid,
        ),
        test(
            "models.list",
            "Models list",
            "models",
            5,
            &[Profile::Core],
            true,
            TestKind::ModelsList,
        ),
        test(
            "models.retrieve",
            "Model retrieve",
            "models",
            2,
            &[Profile::Core],
            false,
            TestKind::ModelsRetrieve,
        ),
        test(
            "chat.basic",
            "Basic chat completion",
            "chat",
            10,
            &[Profile::Core],
            true,
            TestKind::ChatBasic,
        ),
        test(
            "chat.system_message",
            "System message",
            "chat",
            4,
            &[Profile::Core],
            true,
            TestKind::ChatSystemMessage,
        ),
        test(
            "chat.multi_turn",
            "Multi-turn context",
            "chat",
            4,
            &[Profile::Core],
            true,
            TestKind::ChatMultiTurn,
        ),
        test(
            "chat.parameters",
            "Common chat parameters",
            "chat",
            4,
            &[Profile::Core],
            true,
            TestKind::ChatParameters,
        ),
        test(
            "chat.usage",
            "Usage object",
            "usage",
            5,
            &[Profile::Core],
            false,
            TestKind::ChatUsage,
        ),
        test(
            "chat.stream",
            "Chat streaming",
            "streaming",
            15,
            &[Profile::Core],
            true,
            TestKind::ChatStream,
        ),
        test(
            "chat.stream_usage",
            "Streaming usage",
            "streaming",
            3,
            &[Profile::Core],
            false,
            TestKind::ChatStreamUsage,
        ),
        test(
            "errors.invalid_model",
            "Invalid model error",
            "errors",
            3,
            &[Profile::Core],
            true,
            TestKind::ErrorInvalidModel,
        ),
        test(
            "errors.invalid_json",
            "Invalid JSON error",
            "errors",
            2,
            &[Profile::Core],
            true,
            TestKind::ErrorInvalidJson,
        ),
        test(
            "chat.tool_call",
            "Tool calling",
            "tools",
            10,
            &[Profile::Agent],
            true,
            TestKind::ToolCall,
        ),
        test(
            "chat.tool_result_flow",
            "Tool result flow",
            "tools",
            6,
            &[Profile::Agent],
            true,
            TestKind::ToolResultFlow,
        ),
        test(
            "chat.json_mode",
            "JSON mode",
            "schema",
            5,
            &[Profile::Agent],
            true,
            TestKind::JsonMode,
        ),
        test(
            "chat.structured_output",
            "Structured output",
            "schema",
            10,
            &[Profile::Agent],
            true,
            TestKind::StructuredOutput,
        ),
        test(
            "embeddings.single",
            "Single embedding",
            "embeddings",
            5,
            &[Profile::Data],
            true,
            TestKind::EmbeddingsSingle,
        ),
        test(
            "embeddings.batch",
            "Batch embeddings",
            "embeddings",
            5,
            &[Profile::Data],
            true,
            TestKind::EmbeddingsBatch,
        ),
    ]
}

fn test(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    weight: u32,
    profiles: &'static [Profile],
    required: bool,
    kind: TestKind,
) -> BuiltinTest {
    BuiltinTest {
        id,
        name,
        category,
        weight,
        profiles,
        required,
        kind,
    }
}

fn expand_profiles(profiles: &[Profile]) -> BTreeSet<Profile> {
    let mut expanded = BTreeSet::new();
    for profile in profiles {
        match profile {
            Profile::Core => {
                expanded.insert(Profile::Core);
            }
            Profile::Agent => {
                expanded.insert(Profile::Core);
                expanded.insert(Profile::Agent);
            }
            Profile::Data => {
                expanded.insert(Profile::Data);
            }
            Profile::Multimodal => {
                expanded.insert(Profile::Multimodal);
            }
            Profile::Full => {
                expanded.insert(Profile::Core);
                expanded.insert(Profile::Agent);
                expanded.insert(Profile::Data);
                expanded.insert(Profile::Multimodal);
                expanded.insert(Profile::Full);
            }
            Profile::Destructive => {
                expanded.insert(Profile::Destructive);
            }
        }
    }
    expanded
}

fn parse_json_response(resp: &HttpResponse) -> Result<Value, serde_json::Error> {
    resp.json()
}

fn error_message(resp: &HttpResponse) -> Option<String> {
    let value = resp.json().ok()?;
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn error_details(resp: &HttpResponse) -> Value {
    json!({
        "status": resp.status,
        "error_message": error_message(resp)
    })
}

fn path_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('/', "%2F")
        .replace(' ', "%20")
}

fn primary_profile(profiles: &[Profile]) -> Profile {
    profiles.first().copied().unwrap_or(Profile::Core)
}

fn embedding_model(config: &RunConfig) -> &str {
    config
        .embedding_model
        .as_deref()
        .filter(|model| !model.is_empty())
        .unwrap_or(&config.model)
}

fn tool_call_request(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": "What is the weather in Jakarta? Use the provided tool."
            }
        ],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather by city.",
                    "parameters": {
                        "type": "object",
                        "properties": {"city": {"type": "string"}},
                        "required": ["city"],
                        "additionalProperties": false
                    }
                }
            }
        ],
        "tool_choice": {
            "type": "function",
            "function": {"name": "get_weather"}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FeatureConfig, OutputConfig, ThresholdConfig, TimeoutConfig};
    use crate::types::ReportFormat;

    #[test]
    fn core_registry_excludes_agent_and_data_tests() {
        let tests = registry_for_profiles(&[Profile::Core], None);
        let ids = ids(&tests);

        assert!(ids.contains(&"chat.basic"));
        assert!(ids.contains(&"chat.stream"));
        assert!(!ids.contains(&"chat.tool_call"));
        assert!(!ids.contains(&"embeddings.single"));
    }

    #[test]
    fn agent_registry_expands_to_core_plus_agent() {
        let tests = registry_for_profiles(&[Profile::Agent], None);
        let ids = ids(&tests);

        assert!(ids.contains(&"models.list"));
        assert!(ids.contains(&"chat.tool_call"));
        assert!(ids.contains(&"chat.structured_output"));
        assert!(!ids.contains(&"embeddings.single"));
    }

    #[test]
    fn full_registry_includes_core_agent_and_data_tests() {
        let tests = registry_for_profiles(&[Profile::Full], None);
        let ids = ids(&tests);

        assert!(ids.contains(&"chat.basic"));
        assert!(ids.contains(&"chat.tool_call"));
        assert!(ids.contains(&"embeddings.batch"));
    }

    #[test]
    fn quick_selection_uses_expected_small_test_set() {
        let tests = registry_for_profiles(&[Profile::Core], Some("quick"));
        let ids = ids(&tests);

        assert_eq!(
            ids,
            vec![
                "models.list",
                "chat.basic",
                "chat.usage",
                "chat.stream",
                "errors.invalid_model"
            ]
        );
    }

    #[test]
    fn manual_chat_selection_creates_single_test() {
        let tests = registry_for_profiles(&[Profile::Core], Some("manual_chat:hello"));

        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id(), "chat.manual");
        assert_eq!(tests[0].name(), "Manual chat completion");
        assert!(tests[0].required());
    }

    #[test]
    fn helper_functions_parse_error_and_escape_model_path() {
        let response = HttpResponse {
            status: 400,
            headers: Default::default(),
            body: r#"{"error":{"message":"bad request"}}"#.to_string(),
            latency_ms: 5,
        };

        assert_eq!(error_message(&response).as_deref(), Some("bad request"));
        assert_eq!(path_escape("a/b c%"), "a%2Fb%20c%25");
    }

    #[test]
    fn embedding_model_prefers_embedding_model_and_falls_back_to_chat_model() {
        let mut config = test_config();
        config.embedding_model = Some("embed-model".to_string());
        assert_eq!(embedding_model(&config), "embed-model");

        config.embedding_model = None;
        assert_eq!(embedding_model(&config), "chat-model");
    }

    #[test]
    fn tool_call_request_uses_forced_tool_choice() {
        let request = tool_call_request("chat-model");

        assert_eq!(request["model"], "chat-model");
        assert_eq!(
            request["tool_choice"]["function"]["name"],
            serde_json::Value::String("get_weather".to_string())
        );
        assert_eq!(
            request["tools"][0]["function"]["parameters"]["required"][0],
            serde_json::Value::String("city".to_string())
        );
    }

    fn ids(tests: &[BuiltinTest]) -> Vec<&str> {
        tests.iter().map(|test| test.id()).collect()
    }

    fn test_config() -> RunConfig {
        RunConfig {
            name: None,
            base_url: "http://localhost:8080/v1".to_string(),
            api_key: None,
            api_key_env: "OPENAI_API_KEY".to_string(),
            no_auth: true,
            model: "chat-model".to_string(),
            embedding_model: None,
            profiles: vec![Profile::Core],
            timeouts: TimeoutConfig::default(),
            features: FeatureConfig::default(),
            thresholds: ThresholdConfig::default(),
            output: OutputConfig {
                format: ReportFormat::Terminal,
                path: None,
            },
            concurrency: 4,
        }
    }
}
