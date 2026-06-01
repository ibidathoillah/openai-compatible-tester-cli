# PRD Summary

`openai-compatible-tester-cli` is a standalone Rust CLI for testing whether a provider, gateway, proxy, or local inference server is compatible with OpenAI-style APIs.

Positioning:

```txt
Not just "does the endpoint respond?"
But "can this provider be used in production by SDKs, gateways, and agents?"
```

The MVP implemented here uses binary name `octest` and focuses on:

- Core compatibility: auth/no-auth, models, chat completions, streaming, usage, and error format.
- Agent compatibility: tool calling, tool result flow, JSON mode, and structured output.
- Data compatibility: embeddings single and batch.
- CI-friendly output: exit codes plus terminal, JSON, and Markdown reports.
- Safety: API key redaction by default and no destructive/costly tests in the MVP path.

## Critical PRD Additions

These additions are not all MVP work. They define the compatibility surface needed for the tool to be meaningfully different from a raw HTTP tester.

### 1. SDK Compatibility Mode

Add a command family for testing provider behavior through common SDKs, because many providers pass raw REST tests but fail in real applications.

```bash
octest sdk --sdk openai-python
octest sdk --sdk openai-node
octest sdk --sdk vercel-ai
octest sdk --sdk litellm
octest sdk --sdk langchain
```

Required SDK test coverage:

- `openai-python` `chat.completions.create()`.
- `openai-python` `responses.create()`.
- `openai-node` `chat.completions.create()`.
- Streaming through SDK abstractions.
- Tool calling through SDK abstractions.
- Structured output through SDK abstractions.
- Embeddings through SDK abstractions.

SDK mode should run SDK test fixtures in a controlled subprocess or container boundary, redact all secrets, and produce normal `TestResult` entries. It should be optional and not part of the default MVP run, because it adds language runtime dependencies.

### 2. Provider Dialect and Adapter

Providers differ in small but integration-breaking ways. Add explicit dialect handling instead of scattering provider exceptions through tests.

Supported command shape:

```bash
octest run --dialect openai-strict
octest run --dialect openai-legacy
octest run --dialect azure-openai
octest run --dialect local-llm
```

Config shape:

```yaml
compatibility_mode:
  provider: openai
  dialect: strict

adapter:
  path_prefix: /v1
  auth_header: Authorization
  auth_scheme: Bearer
  token_limit_param: max_tokens
  supports_developer_role: false
  supports_legacy_functions: false
  requires_trailing_slash: false
  custom_headers: {}
```

Adapter responsibilities:

- Normalize `/v1/chat/completions` vs `/chat/completions`.
- Select `max_tokens` vs `max_completion_tokens`.
- Support `system` vs `developer` role policy.
- Support modern `tools` and legacy `functions` tests.
- Support Bearer auth and custom auth headers.
- Preserve strict OpenAI-compatible mode as the default.

### 3. Capability Discovery

Add discovery as a separate command from scoring.

```bash
octest discover --base-url http://localhost:8000/v1 --model llama-3.1 --no-auth
```

Discovery output should answer what is likely supported before a full run:

```txt
Detected:
PASS Chat Completions
PASS Streaming
FAIL Responses API
PASS Embeddings
FAIL Images
WARN Tools accepted but ignored
```

Discovery should:

- Probe endpoint existence and basic schema shape.
- Detect accepted-but-ignored behavior for tools, JSON mode, and token limits where feasible.
- Recommend sensible profiles and testpacks.
- Avoid destructive and costly tests.
- Feed optional hints into `run`, but not replace explicit user-selected profiles.

### 4. Ignored Parameter Detector

Add dedicated behavior tests for parameters that providers often accept but ignore.

Test IDs:

```txt
parameters.max_tokens_enforced
parameters.stop_enforced
parameters.temperature_determinism
parameters.tool_choice_enforced
parameters.json_schema_strict_enforced
parameters.response_format_no_markdown
```

Examples:

- Send `max_tokens=1`; output should be very short or return a clear unsupported error.
- Send `stop=["END"]`; output should stop before or at the stop sequence.
- Send forced `tool_choice`; provider must not answer in plain text.
- Send strict JSON schema; extra properties should fail.
- Send JSON object mode; response should not include markdown fences.

Ignored-parameter detection is a high-value differentiator and should be included before V1.

### 5. Rate Limit and Quota Behavior

Add profile-neutral tests for production readiness.

Test IDs:

```txt
rate_limits.headers
rate_limits.retry_after
rate_limits.burst_small
rate_limits.parallel_requests
rate_limits.429_format
```

Checks:

- `x-ratelimit-limit-requests`.
- `x-ratelimit-remaining-requests`.
- `x-ratelimit-reset-requests`.
- `retry-after`.
- 429 body is valid JSON and has a readable error message.
- Parallel requests either succeed or fail with clear quota/rate-limit errors.

These tests should default to non-destructive, low concurrency, and informational thresholds unless `--strict` or explicit rate-limit thresholds are provided.

### 6. Context Length and Token Limit Tests

Add context-specific commands:

```bash
octest context --model gpt-compatible
octest token-count --model gpt-compatible
```

Coverage:

- Maximum accepted input size estimate.
- Maximum output token behavior.
- Prompt-too-long error shape.
- Token counting endpoint or SDK helper behavior where available.
- Prompt caching metadata if supported.
- Compaction/context-management behavior if exposed by provider.

These tests should be careful with cost and should support `--max-cost-usd`, `--max-input-tokens`, and `--dry-run-cost`.

### 7. Streaming Torture Tests

Add a dedicated streaming reliability testpack.

Test IDs:

```txt
stream.slow_reader
stream.client_disconnect
stream.tool_call_delta
stream.malformed_chunk
stream.empty_delta
stream.done_missing
stream.usage_final_chunk
stream.timeout_first_token
```

Goals:

- Validate provider behavior and CLI parser resilience under real-world stream conditions.
- Distinguish provider defects from CLI parser defects.
- Keep default `core` streaming test lightweight; torture tests run only under explicit profile/testpack.

### 8. Regression Baseline and Diff

Add baseline workflows for CI/CD.

```bash
octest run --output current.json
octest diff baseline.json current.json
octest run --fail-on-regression baseline.json
```

Regression examples:

```txt
Regression detected:
- tool_calling: passed -> failed
- first_token_latency: 900ms -> 2400ms
- structured_output: warning -> failed
```

Diff should compare:

- Test status.
- Feature status.
- Scores.
- Latency metrics.
- First-token latency.
- New unsupported endpoints.
- Newly missing metadata such as `usage` or `finish_reason`.

### 9. Compatibility Certification Mode

Add stricter certification commands:

```bash
octest certify --profile core
octest certify --profile agent
```

Behavior:

- `certify` is strict, deterministic, and CI-oriented.
- No skipped required tests.
- Missing usage, missing `finish_reason`, ignored forced tool choice, ignored strict schema, malformed error body, and missing stream completion are failures.
- Produces a full report even on failure.

Output example:

```txt
Certified Core Compatible: YES
Certified Agent Compatible: NO
Reason: structured output failed
```

### 10. OpenAPI Spec Validator

Add spec validation for providers that publish an OpenAPI document.

```bash
octest spec validate openapi.yaml
octest spec compare openai.yaml provider.yaml
```

Checks:

- Endpoint existence.
- HTTP method compatibility.
- Request schema compatibility.
- Response schema compatibility.
- Error schema compatibility.
- Streaming endpoints marked as partially spec-checkable.

This is not a substitute for runtime tests, but it improves professional gateway/provider validation.

### 11. Test Pack System

Add a first-class testpack registry.

```txt
testpacks/
├── openai-core/
├── openai-agent/
├── openai-responses/
├── openai-realtime/
├── local-llm/
├── azure-openai/
└── provider-custom/
```

Command:

```bash
octest run --testpack openai-agent
```

Testpacks should select tests, defaults, requiredness, scoring weights, dialect assumptions, and skip rules without requiring Rust changes.

### 12. LLM Gateway Mode

Add tests for reverse proxies, gateways, and model routers.

Test IDs:

```txt
gateway.header_passthrough
gateway.request_id_forwarding
gateway.timeout_mapping
gateway.error_mapping
gateway.streaming_passthrough
gateway.model_routing
gateway.fallback_behavior
gateway.auth_redaction
```

Gateway mode should verify that compatibility survives the proxy layer, not only the upstream provider.

### 13. Resource Lifecycle Tracking

For files, uploads, batches, vector stores, evals, fine-tuning, containers, and skills, track resources created by a run.

Report shape:

```json
{
  "created_resources": [
    {
      "type": "file",
      "id": "file-xxx",
      "created_by": "octest",
      "cleanup_status": "deleted"
    }
  ]
}
```

Command:

```bash
octest cleanup report.json
```

Rules:

- Only clean resources created by the same run.
- Use an `octest-` prefix or metadata where the API supports it.
- Cleanup is best-effort and recorded in the report.
- Destructive cleanup requires explicit confirmation or `--destructive`.

### 14. Cost Estimate and Budget Guard

Add cost controls before enabling costly endpoint families.

```bash
octest run --max-cost-usd 0.10
octest run --dry-run-cost
```

Output:

```txt
Estimated test cost:
Core: low
Agent: low
Embeddings: low
Images: high
Audio: medium
Fine-tuning: disabled
```

Rules:

- Costly tests remain disabled by default.
- `--max-cost-usd` blocks execution when estimate exceeds budget.
- `--dry-run-cost` performs no provider calls.
- Unknown pricing should be reported as unknown, not guessed.

### 15. Model Capability Matrix

Add per-model matrix support because one provider can expose models with different capabilities.

```bash
octest matrix-models --models gpt-a,gpt-b,embed-a
```

Output:

```txt
| Model | Chat | Stream | Tools | JSON Schema | Vision | Embeddings |
|---|---|---|---|---|---|---|
| model-a | PASS | PASS | PASS | FAIL | FAIL | FAIL |
| model-b | PASS | PASS | FAIL | FAIL | PASS | FAIL |
```

The command should reuse normal test results and avoid running irrelevant tests against embedding-only or image-only models.

### 16. Compatibility Levels

Keep named profiles, but add explicit compatibility levels for clearer public reporting.

```txt
L0 - Reachable
L1 - OpenAI Chat Basic
L2 - OpenAI Chat Streaming
L3 - OpenAI Agent Compatible
L4 - OpenAI Data Compatible
L5 - OpenAI Modern Responses Compatible
L6 - OpenAI Multimodal Compatible
L7 - OpenAI Full Platform Compatible
```

Reports should include both profile scores and the highest certified level.

## Problem-Driven Conformance Scan

The tool must explicitly detect "fake OpenAI-compatible" providers: providers that return `200 OK` for common requests but silently ignore parameters, return subtly incompatible shapes, or fail under SDK/parser validation.

The highest-risk production failures are silent successes:

```txt
1. tools accepted but tool_choice ignored
2. response_format accepted but schema not enforced
3. max_tokens accepted but output not limited
4. audio/image input accepted but stripped
5. invalid model silently falls back to another model
6. usage appears in a non-standard location or has inconsistent totals
7. streaming works but is not valid SSE
8. SDK fails to parse even though curl succeeds
```

### Command Shape

Problem scan:

```bash
octest problems \
  --base-url https://api.example.com/v1 \
  --model gpt-compatible
```

Strict conformance:

```bash
octest conformance --profile strict
octest conformance --profile agent-strict
octest conformance --profile sdk-strict
```

Example output:

```txt
OpenAI-Compatible Problem Scan

PASS Basic chat works
PASS Streaming SSE works
FAIL Tool call shape invalid: missing type=function
FAIL Structured output ignored: extra field returned
WARN Unsupported fields are silently ignored
WARN stream_options.include_usage not supported
FAIL invalid model returned 200: possible silent fallback

Risk: HIGH
This provider may work with curl, but can break SDKs and agents.
```

### Observed Compatibility Failure Modes

These are real-world problem categories that should drive test design. The specific provider examples are not product judgments; they show why raw HTTP success is insufficient.

| Problem | Common manifestation | Required tester response |
|---|---|---|
| Unsupported fields silently ignored | Provider accepts unsupported fields but does nothing with them | Add ignored-parameter detection and behavior assertions |
| Structured output not truly strict | `json_schema` is accepted, but output has wrong types or extra fields | Enforce strict JSON Schema, not just valid JSON |
| Tool-call shape incompatible with strict SDKs | Missing fields such as tool call `type`, invalid `function.arguments`, or unsupported `strict` | Validate strict OpenAI tool-call shape and SDK parseability |
| Streaming tool calls broken | Tool-call deltas absent, malformed, or not streamed after a tool call | Add streaming tool-call delta and after-tool-result tests |
| Usage missing or misplaced | Usage omitted from streams, placed inside `choices`, or inconsistent | Check usage location, streaming usage, and accounting consistency |
| Token-limit dialect mismatch | `max_tokens` vs `max_completion_tokens` varies by model family | Add dialects: `legacy`, `modern`, `reasoning` |
| Model listing and invalid model behavior broken | `/models` missing, target model absent, invalid model silently falls back | Test model existence and invalid-model no-fallback |
| Responses API differs from Chat Completions | Stateless Responses, unsupported `previous_response_id`, or path adapter confusion | Separate chat-compatible and responses-compatible profiles |
| Multimodal input silently stripped | Images/audio accepted but ignored or removed | Assert multimodal input is reflected or rejected clearly |
| Parameter behavior ignored | `presence_penalty`, `frequency_penalty`, `logit_bias`, `n`, `stop`, or temperature accepted but ignored | Test parameter effects, not only status code |
| Error shape differs by API family | Error bodies vary across OpenAI, Anthropic-like, and Responses-like layers | Normalize error parsing and test OpenAI-like error shape |

### Required Problem-Driven Test IDs

Add these as first-class conformance tests:

```txt
compat.silent_ignore.detect
compat.invalid_model_no_fallback
compat.max_tokens_vs_max_completion_tokens
compat.usage_top_level
compat.usage_token_accounting_consistent
compat.stream_include_usage
compat.tool_call_shape_strict
compat.tool_call_stream_delta
compat.tool_strict_supported
compat.response_format_json_schema_enforced
compat.response_format_no_markdown_fence
compat.multimodal_not_silently_stripped
compat.image_parts_supported
compat.input_audio_not_silently_ignored
compat.n_parameter_behavior
compat.logprobs_behavior
compat.error_shape_openai_like
compat.responses_stateful_support
compat.sdk_vercel_ai_parse
compat.sdk_openai_python_parse
compat.sdk_openai_node_parse
```

### Silent-Ignore Detection Rules

The conformance scan must prefer behavior checks over acceptance checks.

Rules:

- A provider that accepts `tool_choice` but returns a plain answer should fail `compat.tool_call_shape_strict`.
- A provider that accepts strict JSON Schema but returns extra fields should fail `compat.response_format_json_schema_enforced`.
- A provider that accepts `max_tokens=1` but returns a long answer should fail `compat.silent_ignore.detect`.
- A provider that accepts image/audio input but produces no evidence it was processed should warn or fail depending on declared features.
- A provider that returns `200` for a definitely invalid model should fail `compat.invalid_model_no_fallback`.
- A provider that returns usage outside the expected top-level object should fail or warn based on dialect, and must be called out as SDK-risky.

### SDK Strict Parse Tests

The conformance suite must include parser-level checks because many failures only appear in SDKs and agent frameworks.

Minimum SDK parse targets:

- OpenAI Python SDK.
- OpenAI Node SDK.
- Vercel AI SDK strict stream/tool parser.
- LiteLLM.
- LangChain where practical.

SDK tests should validate:

- Chat completion response parse.
- Streaming chunk parse.
- Tool call parse.
- Tool-call streaming delta parse.
- Structured output parse.
- Embedding response parse.

If SDK runtimes are unavailable, `octest` should mark SDK tests as `skipped` with an actionable installation message, not silently omit them.

### Chat vs Responses Compatibility

Do not collapse Chat Completions and Responses into one score.

Profiles:

```txt
chat-compatible
responses-compatible
agent-compatible
sdk-compatible
strict-conformance
```

Responses tests must separately check:

- `responses.create`.
- Stateful behavior such as `previous_response_id`, if claimed.
- Output item shape.
- Streaming event names and final event.
- Adapter confusion where Responses payloads are incorrectly sent to Chat Completions endpoints.

### Risk Classification

Problem scan should produce a risk level independent from compatibility score:

```txt
LOW      No silent-ignore or SDK-risky failures found
MEDIUM   Optional behavior warnings or unsupported optional features
HIGH     Silent fallback, schema/tool/stream usage failures, or SDK parse failure
CRITICAL Secret leak, destructive safety failure, invalid auth accepted, or severe protocol corruption
```

Risk level should be included in JSON reports:

```json
{
  "risk": {
    "level": "high",
    "reasons": [
      "invalid model returned 200",
      "forced tool_choice ignored",
      "strict JSON schema not enforced"
    ]
  }
}
```

### Conformance References

Keep this section source-driven and update it as provider compatibility layers evolve. Initial observed-problem references:

- Claude OpenAI SDK compatibility notes: https://platform.claude.com/docs/en/api/openai-sdk
- OpenAI Structured Outputs guide: https://developers.openai.com/api/docs/guides/structured-outputs
- vLLM strict streaming tool-call shape issue: https://github.com/vllm-project/vllm/issues/16340
- Ollama OpenAI compatibility and Responses notes: https://docs.ollama.com/api/openai-compatibility
- Poe OpenAI-compatible API limitations: https://creator.poe.com/docs/external-applications/openai-compatible-api
- MiniMax OpenAI API compatibility notes: https://platform.minimax.io/docs/api-reference/text-openai-api
- Kimi/Moonshot usage compatibility discussion: https://forum.moonshot.ai/t/api-not-fully-openai-compatible/67
- LocalAI error shape reference: https://localai.io/reference/api-errors/

## Performance Profile and Benchmarking

Performance must be a separate profile and report dimension. Compatibility answers whether the API follows OpenAI-style formats. Performance answers whether the API is fast, stable, and scalable enough for production use.

Do not mix compatibility score and performance score. Reports should be able to say:

```txt
Compatibility: 86/100
Performance:   74/100
Production:    usable with warnings
```

### Command Family

Quick performance test:

```bash
octest perf \
  --base-url https://api.example.com/v1 \
  --model gpt-compatible
```

Repeated benchmark:

```bash
octest benchmark \
  --base-url https://api.example.com/v1 \
  --model gpt-compatible \
  --requests 50 \
  --concurrency 5
```

Streaming benchmark:

```bash
octest benchmark stream \
  --base-url https://api.example.com/v1 \
  --model gpt-compatible \
  --requests 20 \
  --concurrency 3
```

Context benchmark:

```bash
octest benchmark context \
  --base-url https://api.example.com/v1 \
  --model gpt-compatible \
  --input-tokens 1000,4000,8000,16000
```

Embeddings benchmark:

```bash
octest perf embeddings \
  --embedding-model text-embedding-compatible \
  --batch-size 1,8,32,128
```

Regression check:

```bash
octest perf --output perf-current.json
octest perf-diff perf-baseline.json perf-current.json
octest benchmark --baseline baseline.json --fail-on-regression
```

### Two Performance Surfaces

Provider/API performance:

- Latency.
- First-byte latency.
- First-token latency / TTFT.
- Tokens per second.
- Stream stability.
- Throughput.
- Error rate.
- Timeout rate.
- Rate-limit behavior.
- Cold start.
- Warm request latency.
- Large context handling.
- Parallel request handling.

CLI self-performance:

- Startup time.
- Memory usage.
- CPU usage.
- Stream parser overhead.
- Report generation time.
- Parallel test efficiency.

CLI self-performance should be measured in internal benchmarks and optionally exposed through `octest self-benchmark`, but it must not affect provider compatibility scoring.

### Performance Profile

Add profile:

```txt
performance
```

Coverage:

```txt
perf.chat_latency
perf.chat_stream_ttft
perf.chat_tokens_per_second
perf.parallel_requests
perf.context_scaling
perf.embedding_latency
perf.error_rate
perf.rate_limit_behavior
perf.cold_start
perf.warm_request
```

Default acceptance:

- Performance tests do not fail only because a provider is slow.
- They fail on timeout, malformed response, missing required stream events, or threshold violations.
- User-defined thresholds convert slow/unstable behavior into failure.

### Required Metrics

Latency metrics:

```txt
p50 latency
p90 latency
p95 latency
p99 latency
min latency
max latency
average latency
```

Use percentiles prominently. Average alone is not acceptable for production readiness.

Streaming metrics:

```txt
TTFT / time to first token
time to first byte
chunk count
tokens per second
stream duration
stream disconnect rate
missing DONE rate
malformed chunk rate
```

Throughput metrics:

```txt
requests per second
successful requests per second
tokens per second total
max stable concurrency
error rate under load
timeout rate
rate limit rate
```

Error metrics:

```txt
2xx rate
4xx rate
5xx rate
429 rate
timeout rate
connection reset rate
malformed response rate
```

### Performance Test Cases

`perf.chat_latency` sends the standard `Reply with exactly: pong` chat request and records:

- `latency_ms`.
- `status_code`.
- `response_size_bytes`.
- `usage.prompt_tokens`.
- `usage.completion_tokens`.
- `usage.total_tokens`.

`perf.stream_ttft` sends a streaming prompt and records:

- `first_byte_ms`.
- `first_token_ms`.
- `total_duration_ms`.
- `chunk_count`.
- `estimated_output_tokens`.
- `tokens_per_second`.

`perf.parallel_requests` runs a small concurrency test:

```bash
octest perf --requests 30 --concurrency 5
```

Metrics:

- `success_count`.
- `failed_count`.
- `timeout_count`.
- `rps`.
- `p50`.
- `p95`.
- `p99`.

`perf.context_scaling` measures prompt-size scaling:

```bash
octest perf context --input-tokens 1000,4000,8000,16000
```

Output example:

```txt
Context Scaling:
  1k tokens    p95 1.2s    TTFT 600ms
  4k tokens    p95 2.8s    TTFT 1.1s
  8k tokens    p95 5.6s    TTFT 2.4s
  16k tokens   failed: context length exceeded
```

`perf.embedding_latency` measures:

- Latency per batch size.
- Vectors per second.
- Embedding dimension.
- Error rate.

### Cold Start vs Warm Request

Add:

```bash
octest perf --cold-warm
```

The test must separate:

- First cold request.
- Second warm request.
- Third warm request.

Output example:

```txt
Cold/Warm:
  Cold request    8.2s
  Warm p50        920ms
  Warm p95        1.4s
```

This is especially important for local LLM servers and serverless providers.

### Performance Scoring

Keep performance score separate:

```json
{
  "compatibility_score": 86,
  "performance_score": 74
}
```

Suggested weights:

```txt
Latency p95             25
First-token latency     25
Tokens per second       20
Error rate              15
Concurrency stability   10
Streaming stability      5
```

Performance grade:

```txt
90-100  Excellent
75-89   Good
60-74   Acceptable
40-59   Slow / unstable
0-39    Not production ready
```

### Threshold Config

Add YAML config:

```yaml
performance:
  requests: 50
  concurrency: 5
  warmup_requests: 3

  thresholds:
    max_p95_latency_ms: 5000
    max_p99_latency_ms: 10000
    max_ttft_ms: 2500
    min_tokens_per_second: 20
    max_error_rate: 0.05
    max_timeout_rate: 0.02

regression:
  fail_if_latency_regression_percent: 30
  fail_if_ttft_regression_percent: 30
  fail_if_error_rate_increase_percent: 5
```

Example threshold command:

```bash
octest perf \
  --requests 50 \
  --concurrency 5 \
  --max-error-rate 0.05 \
  --max-p95-latency-ms 5000 \
  --max-ttft-ms 2000
```

### Performance Report Shape

JSON report:

```json
{
  "performance": {
    "chat": {
      "requests": 50,
      "concurrency": 5,
      "success_rate": 0.98,
      "error_rate": 0.02,
      "latency_ms": {
        "p50": 820,
        "p90": 1400,
        "p95": 1800,
        "p99": 3200,
        "max": 4100
      }
    },
    "streaming": {
      "first_token_latency_ms": {
        "p50": 740,
        "p95": 1600
      },
      "tokens_per_second": {
        "p50": 42.5,
        "p95": 31.2
      },
      "malformed_chunk_rate": 0.0
    }
  }
}
```

Markdown report:

```md
## Performance Summary

| Metric | Result |
|---|---:|
| p50 latency | 820ms |
| p95 latency | 1.8s |
| p99 latency | 3.2s |
| TTFT p95 | 1.6s |
| Tokens/sec p50 | 42.5 |
| Error rate | 2% |
| Success rate | 98% |
```

### Performance Regression

Performance regression output:

```txt
Performance regression detected:

TTFT p95:
  baseline: 900ms
  current : 1800ms
  change  : +100%

Chat p95:
  baseline: 1.4s
  current : 2.7s
  change  : +92%

Status: FAILED
```

### Performance Safety Limits

`octest` is not a replacement for k6, wrk, vegeta, or Locust. It performs compatibility-aware lightweight benchmarking only.

Defaults:

```txt
requests default: 20
concurrency default: 2
max concurrency without explicit override: 20
```

High concurrency must require an explicit safety flag:

```bash
octest benchmark --concurrency 100 --i-know-what-im-doing
```

### Performance Requirements

1. CLI must measure latency percentiles: p50, p90, p95, p99.
2. CLI must measure streaming first-token latency.
3. CLI must estimate output tokens per second.
4. CLI must support configurable concurrency.
5. CLI must separate warmup requests from measured requests.
6. CLI must report error rate, timeout rate, and 429 rate.
7. CLI must support performance regression comparison.
8. CLI must support user-defined thresholds.
9. CLI must not run high-concurrency tests by default.
10. CLI must keep compatibility score and performance score separate.

### Performance Implementation Priority

1. Basic latency p50/p95/p99.
2. Streaming TTFT.
3. Tokens/sec estimation.
4. Warmup requests.
5. Small concurrency.
6. Error rate.
7. Threshold failure.
8. Performance JSON report.
9. Baseline diff.
10. Context scaling.

## Production Readiness and Maturity Additions

Beyond coverage, SDK compatibility, and performance, the tool should mature into a conformance, regression, and production-readiness suite.

### Security Testing

Add safety tests for gateways, proxies, and hosted providers.

Test IDs:

```txt
security.api_key_redaction
security.auth_required
security.invalid_auth
security.header_leak
security.prompt_in_log_leak
security.error_secret_leak
security.cors_optional
security.tls_info
security.insecure_http_warning
```

Example warnings:

```txt
WARN Provider returns API key fragment in error response.
WARN Base URL uses HTTP, not HTTPS.
WARN Authorization header appears in debug dump.
```

### Token Accounting Accuracy

Usage metadata must be checked for consistency, not only presence.

Test IDs:

```txt
usage.prompt_tokens_exists
usage.completion_tokens_exists
usage.total_tokens_consistent
usage.total_tokens_equals_prompt_plus_completion
usage_stream_matches_final_usage
```

Mismatch example:

```txt
usage.total_tokens mismatch
prompt_tokens: 20
completion_tokens: 15
total_tokens: 999
```

### Cost and Billing Sanity

Add optional billing metadata checks for commercial providers.

```bash
octest billing --base-url ... --model ...
```

Potential metadata:

```txt
x-request-cost
x-usage-cost
x-credits-remaining
```

This is informational and not required for OpenAI compatibility.

### Protocol-Level Compatibility

Validate HTTP behavior beyond JSON bodies.

Test IDs:

```txt
protocol.http_status_codes
protocol.content_type_json
protocol.content_type_sse
protocol.keep_alive
protocol.gzip
protocol.http2
protocol.timeout_behavior
protocol.connection_reuse
protocol.request_id_header
```

### Network Resilience

Add V2/V3 resilience checks for poor network conditions.

```bash
octest resilience --base-url ... --model ...
```

Test IDs:

```txt
network.timeout
network.retry_after
network.connection_reset
network.slow_stream
network.partial_response
network.large_response
```

### Safe Fuzzing and Negative Testing

Add safe fuzz profile:

```bash
octest fuzz --profile safe
```

Requests:

- Empty messages.
- Missing model.
- Wrong message role.
- Invalid JSON.
- Huge temperature.
- Negative `max_tokens`.
- Invalid `response_format`.
- Malformed tools schema.
- Duplicate tool names.
- Unicode prompt.
- Very long system message.

The provider should return clear 4xx errors, not 500s or malformed responses.

### Model Behavior Sanity and Multilingual Compatibility

Behavior sanity is not a semantic quality benchmark. It checks whether basic instruction-following and encoding are intact.

Test examples:

- Exact instruction following.
- JSON response.
- Forced tool use.
- Stop sequence.
- `max_tokens`.
- Unicode.
- Indonesian prompt.
- Japanese.
- Arabic.
- Emoji.
- Mixed unicode.

### Schema Drift Detection

Add schema diff for regression reports:

```bash
octest schema-diff baseline.json current.json
```

Output example:

```txt
Schema drift detected:
- $.choices[0].message.content removed
- $.usage added
- $.choices[0].finish_reason type changed: string -> null
```

### Canary Monitoring and SLO

Add periodic health-check mode:

```bash
octest canary --config provider.yaml --interval 60s
```

SLO config:

```yaml
slo:
  min_compatibility_score: 90
  max_p95_latency_ms: 3000
  max_error_rate: 0.01
  max_timeout_rate: 0.005
  required_features:
    - chat
    - streaming
    - tools
    - structured_output
```

Output example:

```txt
Production Readiness: FAILED

Reasons:
- p95 latency exceeded SLO
- structured output failed
```

### Provider Claim Verification

Verify declared features against actual behavior.

```yaml
declared_features:
  chat: true
  streaming: true
  tools: true
  embeddings: false
  images: false
```

Output:

```txt
Claimed vs Actual:
  chat       claimed PASS actual PASS
  streaming  claimed PASS actual PASS
  tools      claimed PASS actual FAIL
```

### Compatibility Contract File

Add contract testing:

```yaml
contract:
  name: openai-agent-compatible
  required:
    - models.list
    - chat.basic
    - chat.stream
    - chat.tool_call
    - chat.structured_output
  thresholds:
    min_score: 90
    max_p95_latency_ms: 3000
```

Command:

```bash
octest contract verify openai-agent.yaml
```

### Golden Response Snapshots

Add snapshot workflows for provider/gateway regressions.

```bash
octest snapshot create --output snapshots/core.json
octest snapshot verify snapshots/core.json
```

Compare:

- Status code.
- Important headers.
- JSON shape.
- Stream event shape.
- Error shape.
- Usage shape.

### Debug Dump, Replay, and Exports

Add developer-friendly failure analysis.

```bash
octest run --debug
octest run --dump-dir ./octest-debug
octest replay ./octest-debug/chat-tool-call.json
octest run --export-curl failed.sh
octest issue report.json --format github
```

Debug dump must save sanitized request, sanitized response, stream chunks, timing breakdown, and a replayable failed request. HAR/curl export must redact secrets.

### API Target Versioning

Add target API families and versions:

```yaml
api_target:
  family: openai
  version: "2026-06"
```

Commands:

```bash
octest run --target openai-2026-06
octest run --target openai-legacy-chat
```

Testpacks should be versioned so API evolution does not break older provider validation.

### Azure and Local LLM Presets

Azure OpenAI remains a dialect, not the default:

```bash
octest run --dialect azure-openai
```

Differences to model:

- Deployment name.
- `api-version` query parameter.
- Endpoint path.
- Auth header.

Local LLM presets:

```bash
octest run --preset vllm
octest run --preset ollama-openai
octest run --preset llama-cpp
octest run --preset lmstudio
```

Preset defaults:

```yaml
preset: local-llm
no_auth: true
skip:
  - images
  - audio
  - files
  - batches
```

### Documentation and Open Source Readiness

Required docs:

- Quickstart.
- Config reference.
- Profile explanation.
- Scoring explanation.
- CI example.
- Troubleshooting.
- Provider examples.
- How to add a test case.
- How to read reports.
- Security/redaction explanation.

Required repo trust files before public release:

- `LICENSE`.
- `CONTRIBUTING.md`.
- `CODE_OF_CONDUCT.md`.
- `SECURITY.md`.
- `CHANGELOG.md`.
- Issue templates.
- PR template.
- Compatibility report template.

### Extensibility

Prefer testpack folders before a binary plugin system:

```txt
~/.octest/testpacks/
```

Future commands:

```bash
octest plugin list
octest plugin install provider-x
octest run --plugin provider-x
```

Plugin marketplace is explicitly deferred.

## Future Endpoint Coverage Registry

The test registry should be designed so new endpoint families can be added without large architectural rewrites.

Endpoint families to reserve:

- Responses API.
- Conversations API.
- Chat Completions API.
- Streaming events.
- Realtime API: WebSocket, WebRTC, SIP, transcription, tools, and server-side controls.
- Webhooks.
- Audio.
- Video.
- Images.
- Embeddings.
- Evals.
- Fine-tuning.
- Batches.
- Files.
- Uploads.
- Models.
- Moderations.
- Vector stores.
- Containers.
- Skills.
- Admin APIs.
- Workload identity tokens.
- Prompt caching.
- Token counting.
- Context compaction/context management.

Not all endpoint families should be tested by default. Each family needs requiredness, cost, destructive behavior, supported profiles, and cleanup policy.

## Realtime Profile

Add a future profile:

```txt
realtime
```

Coverage:

- WebSocket connection.
- Session create/update.
- Client event send.
- Server event receive.
- Realtime transcription.
- Realtime tools.
- Disconnect handling.
- Reconnect handling.
- Optional WebRTC/SIP paths in later versions.

Realtime is explicitly not MVP. Target V2/V3 after core REST, SDK, and regression workflows are stable.

## Updated Roadmap

### MVP Already Implemented

- CLI binary `octest`.
- Config via flags and YAML.
- Models list/retrieve.
- Chat basic/system/multi-turn/parameters.
- Streaming.
- Usage and error checks.
- Tool calling.
- Tool result flow.
- JSON mode and structured output.
- Embeddings single/batch.
- Terminal, JSON, and Markdown reports.
- Secret redaction.
- Exit codes.
- Mock server.

### Next Priority: Production Compatibility

1. SDK compatibility mode.
2. Provider dialect/adapter.
3. Capability discovery.
4. Problem-driven conformance scan for fake OpenAI-compatible providers.
5. Ignored parameter detector.
6. Performance profile: p50/p95/p99 latency, TTFT, tokens/sec, warmup, small concurrency, error rate, thresholds, JSON report, perf diff.
7. Rate limit behavior.
8. Context length/token limit tests.
9. Regression baseline/diff.
10. Security/redaction testing.
11. Token usage accuracy.
12. Protocol-level compatibility.
13. Provider claim verification.
14. Contract testing.
15. Debug dump and replay.
16. Resource lifecycle cleanup.
17. Cost guard.
18. Local LLM presets and Azure dialect.
19. Realtime/WebSocket profile design for V2/V3.

### Defer

- Video API runtime tests.
- Evals runtime tests.
- Fine-tuning create-job tests.
- Realtime WebRTC/SIP runtime tests.
- Admin APIs.
- Containers.
- Skills.
- Voice consent workflows.
- Public leaderboard.
- TUI.
- Plugin marketplace.

## Official API Coverage Notes

The future registry should track OpenAI's public API reference and guides over time. As of the 2026-06-02 planning update, the official docs include API reference pages and guides for modern platform surfaces such as Responses, Conversations, streaming, Webhooks, Realtime, images, audio, videos, embeddings, evals, fine-tuning, batches, files/uploads, models, moderations, vector stores, containers, and rate-limit headers. Keep this section updated from official OpenAI docs before implementing each new endpoint family.

References:

- https://platform.openai.com/docs/api-reference
- https://platform.openai.com/docs/guides/realtime
- https://platform.openai.com/docs/guides/rate-limits
- https://platform.openai.com/docs/guides/prompt-caching
- https://platform.openai.com/docs/guides/conversation-state
