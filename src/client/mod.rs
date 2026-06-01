use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::{header::AUTHORIZATION, Method};
use serde_json::Value;
use thiserror::Error;

use crate::config::RunConfig;
use crate::util::redact::redact_secret;

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    no_auth: bool,
    redact: bool,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub latency_ms: u128,
}

impl HttpResponse {
    pub fn json(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_str(&self.body)
    }
}

#[derive(Debug, Clone)]
pub struct StreamResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub delta_content: String,
    pub done_received: bool,
    pub usage_seen: bool,
    pub chunk_count: usize,
    pub bytes_received: usize,
    pub first_token_latency_ms: Option<u128>,
    pub total_stream_duration_ms: u128,
    pub raw_events: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("request timed out: {0}")]
    Timeout(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("request build error: {0}")]
    Build(String),
}

impl ClientError {
    pub fn is_timeout(&self) -> bool {
        matches!(self, ClientError::Timeout(_))
    }
}

impl ApiClient {
    pub fn new(config: &RunConfig) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(config.timeouts.connect_ms))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|err| ClientError::Build(err.to_string()))?;

        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key.clone(),
            no_auth: config.no_auth,
            redact: config.features.redact,
        })
    }

    pub async fn get(&self, endpoint: &str, timeout_ms: u64) -> Result<HttpResponse, ClientError> {
        self.request(Method::GET, endpoint, None, timeout_ms).await
    }

    pub async fn post_json(
        &self,
        endpoint: &str,
        body: Value,
        timeout_ms: u64,
    ) -> Result<HttpResponse, ClientError> {
        self.request(Method::POST, endpoint, Some(body), timeout_ms)
            .await
    }

    pub async fn post_raw(
        &self,
        endpoint: &str,
        body: String,
        content_type: &str,
        timeout_ms: u64,
    ) -> Result<HttpResponse, ClientError> {
        let start = Instant::now();
        let mut request = self
            .http
            .post(self.url(endpoint))
            .timeout(Duration::from_millis(timeout_ms))
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body);
        request = self.apply_auth(request);

        let response = request.send().await.map_err(|err| self.map_reqwest(err))?;
        let status = response.status().as_u16();
        let headers = collect_headers(response.headers());
        let body = response.text().await.map_err(|err| self.map_reqwest(err))?;

        Ok(HttpResponse {
            status,
            headers,
            body,
            latency_ms: start.elapsed().as_millis(),
        })
    }

    pub async fn post_stream(
        &self,
        endpoint: &str,
        body: Value,
        timeout_ms: u64,
    ) -> Result<StreamResponse, ClientError> {
        let start = Instant::now();
        let mut request = self
            .http
            .post(self.url(endpoint))
            .timeout(Duration::from_millis(timeout_ms))
            .json(&body);
        request = self.apply_auth(request);

        let response = request.send().await.map_err(|err| self.map_reqwest(err))?;
        let status = response.status().as_u16();
        let headers = collect_headers(response.headers());
        let mut stream = response.bytes_stream();

        let mut buffer = String::new();
        let mut delta_content = String::new();
        let mut done_received = false;
        let mut usage_seen = false;
        let mut chunk_count = 0usize;
        let mut bytes_received = 0usize;
        let mut first_token_latency_ms = None;
        let mut raw_events = Vec::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|err| self.map_reqwest(err))?;
            bytes_received += bytes.len();
            chunk_count += 1;
            if first_token_latency_ms.is_none() && !bytes.is_empty() {
                first_token_latency_ms = Some(start.elapsed().as_millis());
            }

            buffer.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(line) = take_line(&mut buffer) {
                let line = line.trim();
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    raw_events.push(data.to_string());
                    if data == "[DONE]" {
                        done_received = true;
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<Value>(data) {
                        if value.get("usage").is_some() {
                            usage_seen = true;
                        }
                        if let Some(content) = value
                            .pointer("/choices/0/delta/content")
                            .and_then(Value::as_str)
                        {
                            delta_content.push_str(content);
                        }
                        if let Some(content) = value
                            .pointer("/choices/0/message/content")
                            .and_then(Value::as_str)
                        {
                            delta_content.push_str(content);
                        }
                    }
                }
            }
        }

        Ok(StreamResponse {
            status,
            headers,
            delta_content,
            done_received,
            usage_seen,
            chunk_count,
            bytes_received,
            first_token_latency_ms,
            total_stream_duration_ms: start.elapsed().as_millis(),
            raw_events,
        })
    }

    async fn request(
        &self,
        method: Method,
        endpoint: &str,
        body: Option<Value>,
        timeout_ms: u64,
    ) -> Result<HttpResponse, ClientError> {
        let start = Instant::now();
        let mut request = self
            .http
            .request(method, self.url(endpoint))
            .timeout(Duration::from_millis(timeout_ms));
        request = self.apply_auth(request);
        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request.send().await.map_err(|err| self.map_reqwest(err))?;
        let status = response.status().as_u16();
        let headers = collect_headers(response.headers());
        let body = response.text().await.map_err(|err| self.map_reqwest(err))?;

        Ok(HttpResponse {
            status,
            headers,
            body,
            latency_ms: start.elapsed().as_millis(),
        })
    }

    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.no_auth {
            return request;
        }

        if let Some(api_key) = &self.api_key {
            request.header(AUTHORIZATION, format!("Bearer {api_key}"))
        } else {
            request
        }
    }

    fn url(&self, endpoint: &str) -> String {
        format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'))
    }

    fn map_reqwest(&self, err: reqwest::Error) -> ClientError {
        let message = redact_secret(&err.to_string(), self.api_key.as_deref(), self.redact);
        if err.is_timeout() {
            ClientError::Timeout(message)
        } else {
            ClientError::Network(message)
        }
    }
}

fn collect_headers(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            Some((
                name.as_str().to_ascii_lowercase(),
                value.to_str().ok()?.to_string(),
            ))
        })
        .collect()
}

fn take_line(buffer: &mut String) -> Option<String> {
    let pos = buffer.find('\n')?;
    let line = buffer[..pos].trim_end_matches('\r').to_string();
    let rest = buffer[pos + 1..].to_string();
    *buffer = rest;
    Some(line)
}

#[cfg(test)]
mod tests {
    use reqwest::header::{HeaderMap, HeaderValue};

    use super::{collect_headers, take_line, HttpResponse};

    #[test]
    fn http_response_parses_json_body() {
        let response = HttpResponse {
            status: 200,
            headers: Default::default(),
            body: r#"{"ok":true}"#.to_string(),
            latency_ms: 12,
        };

        assert_eq!(response.json().unwrap()["ok"], true);
    }

    #[test]
    fn collect_headers_lowercases_names_and_skips_invalid_values() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Request-ID", HeaderValue::from_static("req-1"));

        let collected = collect_headers(&headers);

        assert_eq!(
            collected.get("x-request-id").map(String::as_str),
            Some("req-1")
        );
    }

    #[test]
    fn take_line_handles_crlf_and_keeps_partial_tail() {
        let mut buffer = "data: one\r\ndata: two".to_string();

        assert_eq!(take_line(&mut buffer).as_deref(), Some("data: one"));
        assert_eq!(buffer, "data: two");
        assert_eq!(take_line(&mut buffer), None);
    }
}
