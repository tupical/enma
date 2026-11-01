# Enma 閻魔 — decisions layer of Meisei

> **Meisei** 明晰 (“clarity”) is an open pipeline that carries raw intent through
> understanding → decision → plan → action to a finished result.

[![Meisei](https://img.shields.io/badge/meisei-明晰-1f2937.svg)](https://meisei.ru)
[![License: Apache-2.0 WITH Commons-Clause](https://img.shields.io/badge/license-Apache--2.0%20WITH%20Commons--Clause-blue.svg)](LICENSE)

<sub>
torii · satori · <b>enma</b> · yatagarasu · fujin · daruma
&nbsp;—&nbsp; intake · sensemaking · <b>decisions</b> · planning · actions · execution (terminal)
</sub>

## What it is

Enma is the **decisions** layer of the Meisei pipeline: it turns understanding
into *direction*. It owns the goal-and-decision primitives — `Decision` (a
recorded choice with statement, author, rationale, the alternatives weighed,
consequences accepted, and revisit conditions) and `Directive` (six lighter
direction primitives: `goal`, `non_goal`, `constraint`, `assumption`,
`principle`, `success_criteria`). Decisions and directives link back to the
Sensemaking they rest on and forward to the Planning they inform. Decisions
**never execute**: nothing here schedules, runs, or mutates a plan — realising a
choice is Planning's and Actions' job. The crate has no dependency on daruma or
sibling layers; adapters live only inside the host.

## Repository layout

- `src/` — the `enma` library: `Decision`/`Directive` primitives, links,
  `decide_ai`, error types.
- `server/` — `enma-server`, a thin, independently-deployed HTTP/MCP wrapper over
  the library (the axum/tokio scaffold comes from [`layer-kit`](../layer-kit)).
- `deploy/` — release `build.sh` (stamps the git SHA into `/healthz`) and a
  systemd user unit.

## Build & run

```sh
cargo run -p enma-server
# GET  /healthz   — open liveness/version probe
# POST /v1/mcp    — platform-token gated MCP surface:
#                   enma.decide; read methods: enma.list /
#                   enma.list_decisions, enma.get / enma.get_decision
```

For production builds use `deploy/build.sh` so `/healthz` reports the real git SHA
instead of `"dev"`.

## Configuration (env)

| Variable | Default | Purpose |
| --- | --- | --- |
| `ENMA_PORT` | `8092` | HTTP listen port |
| `ENMA_PLATFORM_SECRET` | unset | HMAC key; if unset, `/v1/mcp` is closed |
| `ENMA_VERSION` | crate version | Version reported by `/healthz` |
| `ENMA_DB` | `./enma.db` | SQLite store path (`layer_kit::store::Store`) |
| `OPENAI_API_KEY` | unset | Optional AI fallback for `enma.decide`; without a key the method fails with `ai_not_configured` (see [Errors over MCP](#errors-over-mcp)) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL of the OpenAI-compatible API |
| `OPENAI_MODEL` | `gpt-4.1` | Model used by the AI fallback |

## Errors over MCP

`POST /v1/mcp` speaks MCP (JSON-RPC 2.0) only. A tool that fails is a tool
result, not an HTTP error: `tools/call` answers HTTP 200 with
`result.isError: true`, the error object in `result.structuredContent`
(`error` is the machine-readable code) and the same JSON as text in
`result.content[0].text`. The status a method carries internally — e.g. 503
for `ai_not_configured` — never reaches the wire. Calling `enma_decide`
without an AI provider returns:

```json
{"jsonrpc": "2.0", "id": 1, "result": {
  "content": [{"type": "text", "text": "{\"detail\":\"…\",\"error\":\"ai_not_configured\"}"}],
  "structuredContent": {"detail": "AI provider is not configured", "error": "ai_not_configured"},
  "isError": true}}
```

Protocol faults are JSON-RPC `error` objects, also with HTTP 200 (`-32601`
unknown method, `-32602` `tools/call` without `name`). HTTP status codes are
reserved for the transport: 401 for a missing or invalid platform token or a
closed surface (`auth_disabled`), 400 for a body that is not a JSON-RPC 2.0
request, 202 with an empty body for a notification. The envelope is
implemented once in `layer-kit` (`src/serve.rs`).

## Docs

Pipeline canon and layer contracts: https://meisei.ru/docs

## License

Apache-2.0 WITH Commons-Clause — see [LICENSE](LICENSE) and
[LICENSE.commons-clause.md](LICENSE.commons-clause.md).
