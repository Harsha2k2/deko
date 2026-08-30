# sdk v2 architecture — `sdk_v2` branch

> status: phase 1 done on `sdk_v2` (python core + langgraph + backend `?wait`/`?token`). this doc is the brief reference — see git history for full decisions.

## goal

`pip install deko-guard` is one line: `Deko()` zero-arg reads `DEKO_URL` + `DEKO_API_KEY`, `@deko.guard` wraps any tool, `approved|denied|escalate` is typed. no `submit→poll→forward` boilerplate, no drift between prod and simulate (single `policy_engine`).

## how it works

```
tool() → @deko.guard → intent from __doc__+args, payload=json(args), idempotency=hash(qualname+args)
       → POST /action?wait=true → wait (ws → poll Retry-After) → verdict
       → if approved + auto_forward → POST /forward → return tool result
       → if denied/escalate → raise DekoDeniedError / DekoEscalatedError
```

## package layout

```
sdk/python: deko-guard 2.0.0a0  py.typed  extras [mcp,langgraph,crewai,openai]  py>=3.10
  src/deko_guard/{config,client/{raw,auth,polling,facade},core/{guard,check,errors,types,idempotency},adapters/{langgraph,crewai,openai,mcp},proxy/http_proxy,admin}
  tests/test_guard.py + examples/langgraph_example.py
sdk/typescript: deko-guard 2.0.0-alpha.0  exports {.,/vercel,/mcp}
sdk/shared: openapi.json + egress-blocklist.json (single source)
```

`ts/app.ts` stays admin spa helper.

## core client

`DekoRawClient` (httpx sync+async, handles `201 vs 200` idempotent, `401→Auth`, `429→RateLimited` with `Retry-After`): `create_action`, `get_status`, `forward`, `exchange_token` (jwt auto-refresh 5m before expiry).

`Deko` facade: `check(intent, payload, target_url, priority=5, ...)` + `acheck`, `forward`, `wait_for_verdict`. `wait=True` default hits `POST /action?wait=true&timeout=30` — one round-trip; `wait=False` does classic 2-step. `Deko(api_key=..., base_url=...)` overrides env.

## guard

```python
deko = Deko()
@deko.guard(auto_forward=True, on_denied="raise", on_escalate="raise")
def refund(order_id: str, amount: float): return {"order_id": order_id}
# async: @deko.aguard
v = deko.check(intent="delete all users")  # inspection only, no forward
```

derives `intent` from `__doc__ or qualname + args`, truncates 500 chars char-safe, `idempotency_key` auto.

## adapters (thin, <150 lines each)

- **langgraph** (first, per your pick): `guard_tools(tools, deko)` + `deko_node(tools)` wrapping `ToolNode`
- **crewai**: `guard_crewai_tools`, `patch_crew`
- **openai**: `guard_openai_tools`, `patch_openai`, `guard_function_tool`
- **mcp gate**: `deko-guard mcp --upstream "npx ..."` stdio proxy, `tools/call` → `deko.check(wait=True)` → denied `-32600` / escalate `-32001` / approved forward
- **http proxy**: `deko-guard proxy --port 8080` + `HTTP_PROXY=http://localhost:8080`, mirrors `egress.rs` blocklist locally, `POST /action` gate
- **admin separate**: `from deko_guard.admin import DekoAdmin` (needs `DEKO_ADMIN_PASSWORD`, not for agents)

## verdict delivery

`blocking` (default) → `GET /action/{id}/ws?token=<jwt>` (~300ms median) → fallback `GET /status` honoring `Retry-After:5` + jitter, up to `timeout` (default 30, cap `action_ttl`). webhook only for long `escalate` queues. browser ws uses `?token=` because js cannot set headers.

## backend changes (all shipped, `cargo test` 156 green)

- openapi completeness (`src/routes/mod.rs` adds `batch`, `ws`, `token`)
- `ws ?token=` fallback (`src/middleware/auth.rs`)
- `POST /action?wait=true` sync collapse (`src/routes/actions.rs`)
- `sqlite::memory:?cache=shared` so pool sees processor rows (`src/db.rs`)
- batch polyfill documented, `egress-blocklist.json` single source planned

## security / testing / ship

- keys opaque, never logged, typed `py.typed`/`strict ts`.
- `sdk/python/tests` mocked httpx (7 passed) + live `run_all.py` against real deko (11 actions, policy denies + llm approved now that `gemini-flash-lite-latest` works with your `AQ...` key).
- `deko-guard` 2.0.0 tracks deko server `0.1.x` (`^0.1.8`).

## rollout

phase 1 done: `Deko` + `@guard/@aguard` + langgraph + backend `?wait`/`?token`. phase 2: `openai/crewai` polish. phase 3: `mcp`/`http proxy` hardening. phase 4: `deko init` cli + pypi `2.0.0` publish.

see `sdk/README.md` for 30s quick start and `deko-agents/` for 5 live agents.
