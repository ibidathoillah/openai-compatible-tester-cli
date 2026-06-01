# openai-compatible-tester-cli

**A fast Rust CLI for testing whether an "OpenAI-compatible" API is actually compatible.**

Many providers, gateways, proxies, and local inference servers claim OpenAI compatibility. In practice, compatibility often breaks around streaming, tool calling, structured outputs, usage metadata, error shapes, SDK parsing, and ignored parameters.

```txt
curl tells you the API is alive.
octest tells you whether it behaves like an OpenAI-compatible API.
```

`openai-compatible-tester-cli` ships the `octest` binary: a CI-friendly compatibility tester for developers building LLM gateways, provider APIs, local model servers, and agent infrastructure.

## Why

Raw HTTP checks are not enough. The dangerous failures are usually silent:

- `tool_choice` is accepted but ignored.
- `response_format` returns JSON, but not the requested schema.
- Streaming works, but not as valid SSE.
- `usage` is missing, misplaced, or inconsistent.
- Invalid models silently fall back to another model.
- A request works with `curl`, then fails in an SDK or agent framework.

`octest` is built to catch those compatibility gaps and turn them into repeatable reports.

## Current MVP

Implemented now:

| Area | Coverage |
|---|---|
| Models | `GET /models`, `GET /models/{model}` |
| Chat | Basic, system message, multi-turn, common parameters |
| Streaming | SSE parsing, `[DONE]`, first-token timing, stream usage |
| Agent | Tool calling, tool result flow |
| Schema | JSON mode, strict structured output checks |
| Embeddings | Single and batch input |
| Errors | Invalid model, invalid JSON, readable error shape |
| Reports | Terminal, JSON, Markdown |
| Safety | API key redaction by default |
| Local testing | Built-in mock OpenAI-compatible server |

Planned next: SDK compatibility, problem scan/conformance mode, provider dialects, ignored-parameter detection, performance profile, cost/billing guardrails, regression diff, JUnit/HTML reports, testpacks, and broader endpoint coverage.

## Install

From this checkout:

```bash
cargo install --path .
```

Then:

```bash
octest --help
```

## Quick Local Demo

Start the built-in mock server:

```bash
octest mock-server --port 8080
```

Run a quick compatibility scan:

```bash
octest quick \
  --base-url http://localhost:8080/v1 \
  --model mock-chat \
  --no-auth
```

## Test a Provider

Core compatibility:

```bash
octest run \
  --base-url https://api.example.com/v1 \
  --api-key-env PROVIDER_API_KEY \
  --model gpt-compatible \
  --profile core \
  --output report.json
```

Agent compatibility:

```bash
octest run \
  --base-url https://api.example.com/v1 \
  --api-key-env PROVIDER_API_KEY \
  --model gpt-compatible \
  --profile agent \
  --format markdown \
  --output COMPATIBILITY.md
```

Embeddings:

```bash
octest embeddings \
  --base-url https://api.example.com/v1 \
  --api-key-env PROVIDER_API_KEY \
  --embedding-model text-embedding-compatible
```

## Config File

Create a starter config:

```bash
octest init provider.yaml
```

Run from config:

```bash
octest run -c provider.yaml
```

CLI flags override YAML values. API keys are read from `--api-key`, then `--api-key-env`, and are redacted from output and reports.

## Reports

Supported MVP report formats:

- `terminal`
- `json`
- `markdown`

Convert a saved JSON report:

```bash
octest report report.json --format markdown --output COMPATIBILITY.md
```

Example JSON report shape:

```json
{
  "score": {
    "overall": 100,
    "max": 100,
    "grade": "full_compatible"
  },
  "features": {
    "chat_completions": "passed",
    "streaming": "passed",
    "tool_calling": "passed",
    "structured_outputs": "passed"
  }
}
```

## CI Behavior

Exit codes are designed for automation:

| Code | Meaning |
|---:|---|
| 0 | Required tests passed |
| 1 | At least one required test failed |
| 2 | Invalid config or client setup |
| 5 | Internal CLI error |
| 6 | Score below `--min-score` |

Example:

```bash
octest run \
  --base-url "$OPENAI_COMPAT_BASE_URL" \
  --api-key "$OPENAI_COMPAT_API_KEY" \
  --model "$OPENAI_COMPAT_MODEL" \
  --profile core \
  --format json \
  --output report.json \
  --min-score 80
```

See [examples/github-action.yaml](examples/github-action.yaml) for a GitHub Actions starter workflow.

## Roadmap

The next product milestone is production readiness, not just raw HTTP coverage:

- **SDK compatibility mode** for `openai-python`, `openai-node`, Vercel AI SDK, LiteLLM, and LangChain.
- **Problem-driven conformance scan** for fake OpenAI-compatible behavior: silent fallback, ignored tools/schema/token limits, non-standard usage, and SDK parse failures.
- **Provider dialects and adapters** for OpenAI strict, OpenAI legacy, Azure OpenAI, reasoning models, and local LLM servers.
- **Ignored-parameter detection** for `max_tokens`, `stop`, forced `tool_choice`, JSON mode, strict schemas, and `n`.
- **Performance profile** with p50/p95/p99 latency, TTFT, tokens/sec, small-concurrency stability, warmup requests, thresholds, and perf diff.
- **Cost and billing guardrails** with pricing YAML, dry-run estimates, budget blocking, usage consistency checks, billing metadata checks, and cost regression.
- **Production readiness checks** for rate limits, token accounting, protocol behavior, security/redaction, SLOs, regression snapshots, debug replay, and resource cleanup.

Read [PRD.md](PRD.md) for the full product plan and endpoint coverage registry.

## Development

Run tests:

```bash
cargo test
```

Format:

```bash
cargo fmt --all
```

The current codebase includes focused unit tests for config merging, scoring, report rendering, registry selection, redaction, JSON helpers, and client parsing utilities.

## Security

`octest` treats secrets as sensitive by default:

- API keys are read from flags or environment variables.
- Authorization values are redacted from output.
- Reports are designed to be safe to commit.
- Raw debug dumps are not enabled by default.

Do not run costly or destructive endpoint families without explicit flags when those features are added.
