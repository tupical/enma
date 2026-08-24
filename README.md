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
| `OPENAI_API_KEY` | unset | Optional AI fallback for `enma.decide`; without a key the method answers 503 `ai_not_configured` |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL of the OpenAI-compatible API |
| `OPENAI_MODEL` | `gpt-4.1` | Model used by the AI fallback |

## Docs

Pipeline canon and layer contracts: https://meisei.ru/docs

## License

Apache-2.0 WITH Commons-Clause — see [LICENSE](LICENSE) and
[LICENSE.commons-clause.md](LICENSE.commons-clause.md).
