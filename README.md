# openai-compatible-tester-cli

A Rust CLI for checking whether an API endpoint is actually compatible with OpenAI-style APIs.

`curl` tells you the API is alive. `octest` tells you whether it behaves like an OpenAI-compatible API.

## Install

```bash
cargo install --path .
```

## Quick Test

Start the built-in mock server in one terminal:

```bash
octest mock-server --port 8080
```

Run a local compatibility check in another terminal:

```bash
octest quick \
  --base-url http://localhost:8080/v1 \
  --model mock-chat \
  --no-auth
```

## Core Compatibility Test

```bash
octest run \
  --base-url https://api.example.com/v1 \
  --api-key-env PROVIDER_API_KEY \
  --model gpt-compatible \
  --profile core \
  --output report.json
```

## Agent Compatibility Test

```bash
octest run \
  --base-url https://api.example.com/v1 \
  --api-key-env PROVIDER_API_KEY \
  --model gpt-compatible \
  --profile agent \
  --format markdown \
  --output COMPATIBILITY.md
```

## Embeddings Test

```bash
octest embeddings \
  --base-url https://api.example.com/v1 \
  --api-key-env PROVIDER_API_KEY \
  --embedding-model text-embedding-compatible
```

## Config File

Create a template:

```bash
octest init provider.yaml
```

Run from config:

```bash
octest run -c provider.yaml
```

CLI flags override values from the YAML config. API keys are read from `--api-key`, then `--api-key-env`, and are redacted from output.

## Reports

Supported MVP formats:

- `terminal`
- `json`
- `markdown`

Convert a saved JSON report to Markdown:

```bash
octest report report.json --format markdown --output COMPATIBILITY.md
```

## Exit Codes

| Code | Meaning |
|---:|---|
| 0 | Required tests passed |
| 1 | At least one required test failed |
| 2 | Invalid config or client setup |
| 5 | Internal CLI error |
| 6 | Score below `--min-score` |

## MVP Coverage

Implemented in this repository:

- `GET /models`
- `GET /models/{model}`
- `POST /chat/completions`
- Chat streaming via SSE
- Usage object checks
- Error format checks
- Tool calling
- Tool result follow-up
- JSON mode
- Structured output
- Embeddings single and batch
- Terminal, JSON, and Markdown reports
- YAML config template
- Built-in mock server

Declarative YAML test execution, JUnit XML, HTML, badge, matrix, Docker, Files/Batches/Images/Audio/Responses APIs, and release packaging are planned after the MVP foundation.

## Next Priority

The next product milestone is production readiness, not just raw HTTP coverage:

- SDK compatibility mode for `openai-python`, `openai-node`, Vercel AI SDK, LiteLLM, and LangChain.
- Provider dialect/adapters for OpenAI strict, OpenAI legacy, Azure OpenAI, and local LLM servers.
- Capability discovery before scoring.
- Problem-driven conformance scan for "fake OpenAI-compatible" behavior: silent fallback, ignored tools/schema/token limits, invalid model fallback, non-standard usage, SDK parse failure.
- Ignored-parameter detection for `max_tokens`, `stop`, forced `tool_choice`, JSON mode, and strict schemas.
- Separate performance profile with p50/p95/p99 latency, TTFT, tokens/sec, small-concurrency stability, warmup requests, thresholds, and perf diff.
- Rate-limit, context-length, token-limit, and token accounting behavior.
- Security/redaction, protocol-level compatibility, provider claim verification, and safe fuzzing.
- Contract testing, golden snapshots, debug dumps, replay, and curl/HAR-style exports.
- Regression baseline/diff for CI.
- Resource lifecycle cleanup, cost guards, SLO/production readiness checks, local LLM presets, and Azure dialect.

See [PRD.md](PRD.md) for the full roadmap, future endpoint registry, and deferred scope.
