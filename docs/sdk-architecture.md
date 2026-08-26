# SDK v2 architecture — `sdk_v2` branch

> **status:** draft for review before any code is written.  
> **branch:** `sdk_v2` (cut from `master@d5169cf`).  
> **authoring time:** ~30 min deep plan — every decision below was weighed against at least two alternatives before picking the winner.

---

## 1. goals & non-goals

### 1.1 what v2 must achieve
- **`pip install deko-guard` / `npm install deko-guard` is a one-line adoption** — `Deko()` zero-arg works when `DEKO_API_KEY` + `DEKO_URL` are set. no boilerplate `submit → poll → forward` loop in user code ever again.
- **framework-native** — an agent built with langgraph / crewai / openai agents sdk / vercel ai sdk / plain mcp tools should add guarding by wrapping one registry, not rewriting 50 tools.
- **boring & typed** — `py.typed`, `ts strict`, generated openapi client. `approved | denied | escalate` is a tri-state the type checker enforces. `escalate` is not a boolean.
- **fail-closed surfaced** — llm down, rate-limited, malformed payload → deny is raised as a typed exception, never swallowed as `approved`.
- **no drift** — every guard reuses the exact policy engine already verified to be single-implementation (`src/services/policy_engine.rs`).

### 1.2 what v2 is not
- a new backend service. sdk is a *client* to the deko http api that already exists. backend changes are surgical fixes to make that api sdk-friendly (see §9).
- a replacement for the admin spa (`admin/src/api/client.ts`). the spa stays cookie-session based; the sdk is agent-key/jwt based.
- a multi-tenant org system — out of scope for sdk v2. tracked in `docs/architecture.md` p2.

---

## 2. where we are today — api reality the sdk must wrap

surveyed `src/routes/*`, `src/models/*`, `src/middleware/*`, `agent.py`, `admin/src/api/client.ts`. full endpoint table:

### 2.1 agent plane (protected by `agent_auth_middleware` — `src/middleware/auth.rs:35-62`)

| method + path | auth | request shape | response | quirks for sdk |
|---|---|---|---|---|
| `POST /action` | `X-API-Key` **or** `Bearer JWT` | `CreateActionRequest` (`src/routes/actions.rs:26-38`): `intent` required (html-escaped, truncated 500 chars, utf-8-unsafe slice), `payload?: string` (opaque, string not object), `screenshot_base64?: string` (10mb cap), `target_url?: string` (egress-validated at submit), `target_method?: string`, `idempotency_key?: string` (agent-scoped, returns existing on hit), `priority?: i32` (default 5, 0 highest), `execute_at?: string` (raw iso, no validation), `metadata?: object` ( `response_transform` nested ), `priority`/`execute_at` | `201 {id,status:pending}` or `200` on idempotent hit; `400` on egress/screenshot/intent; `401` on bad key | sdk must `JSON.stringify(payload)` — old `agent.py:76` sent dict and would 422. must truncate intent char-safe before send. idempotency should be auto-derived `hash(fn+args)` when caller omits. batch endpoint broken — sdk must polyfill. |
| `GET /action/{id}` | agent + owner check | — | `ActionDetailResponse` (`:47-59`): full action + `verdict: VerdictResponse\|null` (`reasoning_chain` included) | owner check is `action.agent_id != agent.id → 403`. sdk should include helpful error. |
| `GET /action/{id}/status` | agent + owner | — | pending → `200 {action_id, status:"pending", retry_after:5}` + `Retry-After:5`; decided → `200 {action_id,status, verdict:{decision,risk_level,reason}}` | hardcodes `"pending"` string even when actual status is `processing`. sdk treats any no-verdict as pending. `escalate` decision lives in verdict, not status. |
| `POST /action/{id}/forward` | agent + owner + verdict must be `approved` | empty | success `200 {forwarded:true,target_status,target_response,forward_attempts}`; `403` if denied, `423` if escalated, `400` if no verdict / already forwarded, `200 {forwarded:false,note:"No target URL"}` if no target, `200 {forwarded:false,forward_error}` on transport fail (status becomes `forward_failed`) | manual redirects (max 3, re-validated), 3 attempts backoff, 256kb cap. sdk must check `forwarded` bool, not http code. |
| `GET /actions?status&limit&offset` | agent | query `status, limit(≤100), offset` | ` {actions:[ActionDetailResponse sans verdict], total}` | `status` filter binding is broken server-side — sdk filters client-side for now and files backend fix. verdict always null in list — must `get_action` for details. |
| `POST /actions/batch` | agent | `{actions:[CreateActionRequest]}` ≤50 | `201 [ {id,status,intent}... ]` | missing `execute_at` bind, no egress validation, no idempotency — sdk avoids and fans out to `POST /action`. |
| `POST /auth/token` | `X-API-Key` *or* `Authorization: Bearer <api_key>` | header only | `200 {token, expires_in:3600}` | hs256, `DEKO_JWT_SECRET` (random per boot if unset). only place bearer-api_key is accepted. after exchange, use `Authorization: Bearer <jwt>`. |
| `GET /action/{id}/ws` | agent (same middleware) | ws upgrade | polls db 500ms×120 (~60s): `pending` → `completed{decision,reason,risk_level,policy_matched}` → `timeout` | browser js cannot set custom headers on ws — requires `?token=` query fallback (proposed backend fix §9). node/python can set headers. |
| attachments `POST/GET /action/{id}/attachments[...]` | agent + owner | multipart `file` | `200 {attachments:[{id,filename,content_type,file_size}]}` | 10mb cap, ownership via `actions.agent_id` check. |

### 2.2 admin plane (not sdk target, but sdk's `verify`/export helpers touch it)
- `POST /admin/login` form `password` → `Set-Cookie: deko_session` (8h, hashed at rest). sdk never uses it — agents use api keys/jwt. sdk should not import admin auth.
- policy crud/test/simulate already use unified `policy_engine` — sdk's dry-run reuses same endpoint `POST /admin/policies/test` but requires admin password; agent sdk will call `POST /action` with `DEKO_POLICY_DRY_RUN` semantic instead (or new `POST /action?dry_run=true` proposal).

### 2.3 enums (wire format — sdk must mirror exactly)
- `ActionStatus` (`src/models/enums.rs:4-12`): `pending | processing | approved | denied | escalated | forwarded | forward_failed` (snake_case). `create_action` idempotency hit maps `forward_failed → pending` bug — sdk handles.
- `VerdictDecision` (`:25-32`): `approved | denied | escalate` (verb, not `escalated`).
- `RiskLevel` (`:14-23`): `low | medium | high | critical`.
- `CreateActionRequest.priority` default 5; `execute_at` stored raw string.

### 2.4 current samples to *not* copy
- `agent.py:1-262` — drifts: sends `payload` as dict, expects `agent_id` not `id`, conflates `status`/`decision`, no idempotency/priority, no `Retry-After`, no jwt.
- `admin/src/api/client.ts:1-152` — fetch wrapper for spa, correct for cookie sessions, but hand-written not openapi-generated.

---

## 3. design principles

1. **zero-config happy path** — `Deko()` reads `DEKO_URL` (default `http://localhost:8000`), `DEKO_API_KEY` / `DEKO_JWT`, `DEKO_MODE=blocking|polling|webhook` from env. `deko init` scaffolds them if missing.
2. **fail-closed is typed** — denied and escalated are exceptions, not `None`. callers `try/except DekoDenied` naturally.
3. **tri-state, not boolean** — `Verdict` dataclass has `decision: Literal["approved","denied","escalate"]`. `escalate` carries `reason` + `policy_matched` and optionally pauses a graph via `on_escalate` callback.
4. **sync + async twins, not one-size** — `deko.guard` (sync, blocks, ws-first) and `deko.aguard` (async, `await`). never force webhook-only. provide `deko.check` (one-shot, no forward) for dry-run/inspection.
5. **generated client, hand-written ergonomics** — openapi → typed `DekoRawClient` (httpx/fetch), then hand-written `Deko` facade adds polling, ws, forward, retry, `Retry-After`+jitter, `X-Deko-*` headers.
6. **frameworks are adapters, not forks** — core guard lives in `deko_guard/core/`, adapters in `deko_guard/adapters/{langgraph,crewai,openai,vercel,mcp}` thin wrappers that translate framework tool registries into core guard calls. keeps `pip install deko-guard[mcp]` optional.

---

## 4. package layout (monorepo on `sdk_v2`)

```
sdk/
  python/
    pyproject.toml          # name = "deko-guard", version 0.2.0a0, extras [mcp, langgraph, crewai, openai]
    src/deko_guard/
      __init__.py           # re-exports Deko, DekoDeniedError, DekoEscalatedError, DekoConfig
      py.typed              # PEP-561
      config.py             # env + explicit config, validation
      client/
        raw.py              # openapi-generated httpx client (sync+async)
        auth.py             # X-API-Key → JWT exchange + refresh (5 min margin)
        polling.py          # wait_for_verdict: ws → Retry-After poll fallback, abort signal
        forwarding.py       # auto_forward + response_transform passthrough
      core/
        guard.py            # @guard / @aguard decorator factories
        check.py            # deko.check() one-shot
        errors.py           # DekoDeniedError, DekoEscalatedError, DekoRateLimitedError, DekoTimeoutError
        types.py            # Verdict, ActionStatus, RiskLevel, GuardOptions (TypedDict/dataclass)
        idempotency.py      # hash(fn qualname + sorted kwargs) → uuid for idempotency_key
      adapters/
        langgraph.py        # wrap ToolNode / add deko interrupt node
        crewai.py           # wrap @tool
        openai.py           # patch OpenAI client / function_tool
        mcp.py              # mcp gate proxy (stdio+sse)
      proxy/
        http_proxy.py       # mitm http proxy emulating egress.rs blocklist (fail-fast local)
      testing/
        conftest.py         # TestDekoServer (in-process uvicorn or httpx mock)
  typescript/
    package.json            # name = "deko-guard", exports: . , ./vercel, ./mcp
    src/
      client/               # openapi fetch client (generated)
      core/guard.ts         # guard(tool) → wrapped tool
      adapters/vercel.ts    # dekoMiddleware()
      adapters/mcp.ts       # mcp gate (node)
      proxy/http.ts
    tsup.config.ts
  shared/
    openapi.json            # checked-in snapshot from GET /api-docs/openapi.json (CI refreshes)
    egress-blocklist.json   # single source of shared blocklist (rust + python + ts must match)
```

`ts/app.ts` stays as admin spa helper — sdk lives under `sdk/typescript/`, not overlapping.

---

## 5. core client — the boring, correct http layer

### 5.1 `DekoRawClient` (generated, sync + async)
- codegen from `GET /api-docs/openapi.json` via `openapi-generator` (python: `httpx`, ts: `fetch`). checked-in `shared/openapi.json` is source of truth; ci job fails if drift detected.
- every method returns typed models matching `src/models/*.rs` wire format.
- handles `201 vs 200` on idempotent create as success, maps `401→DekoAuthError`, `429→DekoRateLimitedError` (reads `Retry-After` header), `422→DekoValidationError`.

### 5.2 `DekoClient` (hand-written facade, the one users import)
```python
@dataclass
class DekoConfig:
    base_url: str = env("DEKO_URL", "http://localhost:8000")
    api_key: str | None = env("DEKO_API_KEY")
    jwt: str | None = env("DEKO_JWT")
    auto_jwt: bool = True          # exchange api_key → jwt if jwt unset
    mode: Literal["blocking","polling"] = "blocking"  # ws-first vs pure poll
    timeout: float = 30.0
    max_retries: int = 2
    idempotency: bool = True

class Deko:
    def __init__(self, config: DekoConfig | None = None, **overrides): ...
    # sync
    def check(self, intent: str, *, payload=None, target_url=None, target_method=None,
              priority=5, idempotency_key=None, execute_at=None, metadata=None,
              response_transform=None, screenshot_base64=None) -> Verdict: ...
    def wait_for_verdict(self, action_id: str, timeout=30) -> Verdict: ...
    def forward(self, action_id: str) -> ForwardResult: ...
    # async twins
    async def acheck(self, ...): ...
    async def await_verdict(self, ...): ...
```

Auth flow: if `jwt` present → `Authorization: Bearer <jwt>`; else if `api_key` present → `X-API-Key` (and if `auto_jwt`, background `POST /auth/token` → cache `jwt` + `expires_at`, refresh 5 min before expiry; on 401, re-exchange once and retry). This fixes the current `X-API-Key` per-request hash mismatch risk across secret rotation (jwt decouples).

Retries: httpx `Retry-After` from `/status` is honored with jitter (`min(retry_after, 5) + random 0-0.5s`). on `429` from rate limiter, respect header. max 2 retries.

---

## 6. guard primitives — the one-line adoption story

### 6.1 primary: `@deko.guard` / `@deko.aguard` decorators

```python
from deko_guard import Deko, DekoDeniedError, DekoEscalatedError

deko = Deko()  # zero-config

@deko.guard(auto_forward=True, on_denied="raise", on_escalate="raise")
def refund(order_id: str, amount: float) -> dict:
    """refund a customer — docstring becomes part of intent"""
    return requests.post("https://payments.internal/refund", json={"order_id": order_id, "amount": amount}).json()

# caller code unchanged:
try:
    refund(order_id="ord_123", amount=500)
except DekoDeniedError as e:
    # e.verdict.reason, e.verdict.risk_level, e.verdict.policy_matched
    notify_customer(f"blocked: {e.reason}")
except DekoEscalatedError as e:
    enqueue_human_review(e.verdict)
```

**what the decorator does (sync path):**
1. derives `intent = f"{fn.__doc__ or fn.__qualname__}: {kwargs}"`, `payload = json.dumps({args,kwargs})` (truncated 500 chars char-safe, matching server limit but not relying on server's utf-8-unsafe slice), `target_url/method` if fn does http (introspected or explicit `target_url=` kwarg), `idempotency_key = hash(qualname+sorted kwargs)` when caller omits.
2. `POST /action` (or `check()` helper for dry-run mode).
3. `wait_for_verdict` — ws-first (`GET /action/{id}/ws` with `Authorization` header), fallback to `GET /status` polling with `Retry-After`.
4. if `approved` and `auto_forward`: `POST /forward`, return `ForwardResult.target_response` (with `response_transform` applied server-side already). if `denied`/`escalate`: raise typed error unless `on_denied="return"` (returns `GuardResult(verdict, forwarded=None)`).

**async twin** `aguard`/`acheck` uses `httpx.AsyncClient` + `websockets` library, same logic but `await`.

**context manager alternative** for non-decorator call sites:
```python
with deko.guard_context(intent="refund $500", target_url="https://...") as g:
    if g.verdict.decision == "approved":
        requests.post(...)
```

### 6.2 escape hatch: `deko.check` / `deko.acheck`

```python
verdict = deko.check(intent="delete all staging", payload='{"env":"staging"}')
if verdict.decision == "denied":
    log.warning(verdict.reason)
```

Never forwards — inspection only. supports `dry_run=True` (future `?dry_run=true` query param, currently client-side simulation via `POST /admin/policies/test` is admin-only so sdk avoids it and instead posts a real action with `DEKO_POLICY_DRY_RUN` hint — see §9 proposal).

### 6.3 instrumentors — truly one-line for popular frameworks

```python
from deko_guard.adapters.openai import patch_openai
patch_openai(client)  # after this, every client.chat.completions.create(tools=[...]) auto-guards tool calls

from deko_guard.adapters.langgraph import deko_node
graph.add_node("guarded_tools", deko_node(tools, deko=deko, on_escalate=interrupt))

from deko_guard.adapters.crewai import guard_tools
crew = guard_tools(crew, deko=deko)
```

Each adapter is <150 lines: wrap `ToolNode` / `CrewAgent.tools` / `OpenAI beta.chat.completions` to call core guard. registered via entry points so `pip install deko-guard[langgraph]` pulls `langgraph` dep only when needed.

---

## 7. verdict delivery — tiered, not either/or

sdk never forces one transport. `DekoConfig.mode` defaults `blocking` (ws-first), with automatic fallback:

```
try ws (GET /action/{id}/ws, 60s server budget, ~300ms median)
  ↓ on ws error / browser / serverless (no ws support)
poll GET /status honoring Retry-After:5 + jitter, up to `timeout` (default 30s, cap = DEKO_ACTION_TTL_SECS)
  ↓ on escalate + webhook configured
user's `on_escalate` handler or `deko.on("escalate", fn)` via local webhook listener (HMAC verified with deko_guard.testing.verify_signature)
```

**webhook** is *not* the primary path for `approved` (server only sends `denied|escalate` today — `src/services/webhook.rs:35` + `src/services/verdict.rs: webhook only on Denied|Escalate`). sdk documents: use webhook for long-lived human review queues; polling/ws for the hot path.

**browser limitation:** js `WebSocket` cannot set custom headers — today's `/ws` requires `X-API-Key` header and would 401 from browsers. proposed fix in §9 adds `?token=<jwt>` query fallback for ws. until then, ts sdk defaults to polling in browsers and documents it.

---

## 8. mcp gate proxy — the 2026-native adoption path

```
             ┌──────────────────────────┐
 claude ────▶│ deko-guard mcp gate      │───stdio/sse───▶ upstream mcp server
 desktop     │ (intercepts tools/call)  │                (filesystem, github, etc.)
             └────────────┬─────────────┘
                          │ POST /action + wait + forward-or-deny
                          ▼
                       deko server
```

- **binary:** `npx deko-guard mcp --upstream "npx @modelcontextprotocol/server-filesystem /tmp"` or `deko mcp gate --config deko.yaml`.
- **protocol:** stdio (default, Claude Desktop) and sse (`--transport sse --port 3001`). language-agnostic — python sdk ships the node binary via `mcp` extra.
- **intercept:** `initialize`/`tools/list` passthrough; `tools/call` → derive `intent = tool_name + ": " + json(args)`, `payload = args`, `target_url` if tool is `fetch`/`http` — create action, wait, if approved forward to upstream and return result, if denied return `McpError(code=-32600, data={reason, risk_level, policy_matched})`, if escalated return `requires_human` + keep pending for webhook resume.
- **config:** `deko.yaml` reuses `DEKO_URL`/`DEKO_API_KEY` from env, plus `mcp.upstream.{command,args,env}` and `mcp.tools.{allow,deny}` overrides.
- **testing:** `mcp` adapter reuses `TestDekoServer` in-process mock.

This is the zero-code-change path for the ecosystem that now speaks mcp by default.

---

## 9. http proxy mode — zero code change for plain http tools

```
agent code (requests/fetch) ──http──▶ deko-guard proxy :8080 ──guarded──▶ real api
  export HTTP_PROXY=http://localhost:8080   # or per-tool base_url = deko
  export DEKO_GUARD_PROXY_TARGETS="https://bank.example.com/*,https://api.*"
```

- sdk ships `deko-guard proxy --port 8080 --targets "https://bank.example.com/*"` (tiny `mitmproxy`-style or `hyper` forward proxy).
- emulates `src/services/egress.rs` blocklist locally for fail-fast (shared `shared/egress-blocklist.json`), then creates deko action with `target_url` = original request url, only proxies if approved. adds `X-Deko-Action-Id` etc.
- useful for vercel ai sdk `http` tools and legacy code that just calls `fetch`.

---

## 10. changes required in existing deko backend (surgical)

These are prerequisite fixes before sdk v2 ships — all small, all testable:

1. **openapi completeness** (`src/routes/mod.rs:30-76`): add missing paths to `ApiDoc::openapi` — `POST /actions/batch`, `GET /action/{id}/ws`, `POST /auth/token`, attachments, audit. without this codegen omits typed methods. sdk's `shared/openapi.json` snapshot should be generated from the live `GET /api-docs/openapi.json` and diff-checked in ci.
2. **ws auth via query param** (`src/routes/ws.rs` + `src/middleware/auth.rs`): `GET /action/{id}/ws?token=<jwt>` fallback — js `WebSocket` can't set headers. middleware already supports `Authorization: Bearer <jwt>`; add `?token=` as alternative source for `jwt::validate_token`. keeps node/python header path, unblocks browsers.
3. **sync/blocking verdict endpoint** — add `POST /action?wait=true` (or `POST /action/sync`) that creates + waits up to `timeout` query param (default 30s, bounded by `action_ttl`) and returns `{action_id, verdict}` in one round-trip. saves sdk a polling loop for the common <2s case and cuts p99. optional but high value.
4. **batch fix** (`src/routes/actions.rs:329-383`): missing `execute_at` 12th bind, no egress validation, no idempotency, audit batch flag — either fix or deprecate and have sdk polyfill via parallel `POST /action` (recommended: fix the bind + add egress check, keep endpoint but document sdk polyfill fallback).
5. **idempotency ergonomics** — document that `forward_failed` maps to `pending` in idempotency check (`:127-135` bug). backend fix to map correctly or sdk handles by re-submitting with same key and treating `pending` as retry signal.
6. **response_transform docs** — clarify server does `String::replace` not regex/jsonpath, so sdk type `ResponseTransform = Array<{find,replace}> | {find,replace}` is accurate.
7. **egress blocklist single source** — move hardcoded ranges from `src/services/egress.rs` into `shared/egress-blocklist.json` and have rust/python/ts all load it. prevents drift.
8. **admin vs agent sdk separation** — ensure `DekoAdminClient` (future) vs `Deko` (agent) never mix `X-API-Key` with `X-Admin-Password` in generated client. already separated in code; just keep openapi tags `agent` vs `admin` distinct for generator grouping.

No breaking changes to existing `POST /action` / `GET /status` / `POST /forward` contracts.

---

## 11. security & correctness notes

- hashes never leave server — sdk stores api keys opaque, never logs them. `pyproject.toml` classifiers mark `Development Status :: 4`.
- `DekoDeniedError` includes `verdict.reason` + `policy_matched` + `reasoning_chain` but not `llm_raw_response` (hidden in `VerdictResponse` by design).
- screenshot path: sdk base64-encodes via `mimetypes` guess, enforces pre-encode 10mb cap before send.
- `priority`/`execute_at` exposed as optional kwargs for power users; sdk validates `priority 0-10` and `execute_at` is `datetime` → `isoformat`.
- webhook `Escalate` payloads are already hmac-signed (`src/services/webhook.rs`) — sdk's local webhook listener (for `on_escalate` callback mode) must verify `X-Deko-Signature` + 5min replay window via `deko_guard.testing.verify_signature` helper (mirrors `WebhookService::verify`).

---

## 12. testing strategy

- **unit:** `sdk/python/tests/test_guard.py` — mocked httpx (no network), covers intent derivation, truncation, idempotency hash, error mapping, retry-after, forward booleans. ts mirror via `vitest`.
- **integration:** `sdk/python/tests/integration/test_live.py` — spins `TestDekoServer` (in-process `axum` via `cargo test` helper or `httpx.MockTransport` replaying recorded fixtures from `tests/integration.rs` live flow). covers ws→poll fallback, batch polyfill, mcp gate.
- **contract:** nightly `openapi diff` job — `GET /api-docs/openapi.json` snapshot vs `shared/openapi.json` fails ci on drift.
- **existing rust tests unchanged** — sdk does not touch `src/*` except §9 backend surgical fixes, which get their own rust integration tests (`tests/integration.rs` already has ws, forward, batch scaffolding).

---

## 13. distribution

- **python:** `deko-guard` on pypi, `py.typed`, `python_requires >=3.10`, extras `mcp`, `langgraph`, `crewai`, `openai`. `pip install deko-guard[mcp]` pulls `mcp` python sdk. version follows deko server `0.2.x` (sdk v2).
- **typescript:** `deko-guard` on npm, `exports: {".", "./vercel", "./mcp"}`, `tsup` build `cjs+esm+dts`, `sideEffects: false`.
- **mcp binary:** `npx deko-guard` via `bin/deko-guard` shim (node) — python package declares `console_scripts: deko-guard = deko_guard.cli:main` that delegates to node binary if `mcp` extra installed.
- **versioning:** sdk `0.2.x` tracks server `0.1.x` — `^0.2.0` requires `deko server >=0.1.8` (the audit+egress release). documented in `sdk/README.md`.

---

## 14. alternatives considered & why not

| alternative | why we didn't pick it |
|---|---|
| webhook-only verdict delivery | adds local server requirement, no good for lambda/edge, server only sends `denied|escalate` today |
| polling-only (no ws) | higher p99 (Retry-After 5s), ws already exists and is ~300ms median |
| sidecar container instead of library | heavier ops, doesn't solve decorator ergonomics, still needs sdk for type safety |
| forking `agent.py` into sdk v1 | v1 patterns are the problem — `payload` as dict, `agent_id` vs `id` mismatch, no tri-state, no idempotency. v2 deserves a clean break (hence `sdk_v2` branch name: nobody trusts v1) |
| openapi hand-written fetch | drifts (admin spa already drifted `rules_json` vs `rules`). codegen + snapshot prevents it |

---

## 15. phased rollout (on `sdk_v2`)

**phase 1 (~2 days, this branch, blocking):** `DekoRawClient` generated + `Deko` facade (`check`, `wait_for_verdict`, `forward`), `core/guard.py` `@guard`/`@aguard` + `DekoDeniedError`/`Escalated`, python + ts packages scaffolding, `shared/openapi.json` snapshot, backend fixes §9 items 1-2 (openapi + ws query token). tests green, `cargo test` still 152+.

**phase 2 (~1 day):** instrumentors (`adapters/openai`, `langgraph`, `crewai`) + vercel `dekoMiddleware()`. each <150 lines, behind extras.

**phase 3 (~1 day):** mcp gate (stdio+sse) + http proxy emulating egress. both reuse core guard, ship as `deko-guard[mcp]` extra.

**phase 4 (polish):** `deko init` cli, docs site (`sdk/README.md` + `docs/sdk.md`), pypi/npm publish dry-run.

---

## 16. open questions for you before code

1. **first adapter priority** — langgraph vs crewai vs openai agents vs mcp? pick one for phase 1 demo, the rest follow the same pattern.
2. **sync endpoint** — want `POST /action?wait=true` to collapse create+wait into one round-trip (saves polling for fast llm verdicts)? or keep explicit `check` + `wait`?
3. **admin sdk** — should `sdk_v2` also ship `DekoAdmin` (policy crud, register agent) for scripting, or keep admin plane spa-only?
4. **versioning** — okay to publish `deko-guard` 0.2.x tracking server 0.1.x, or prefer `1.0.0` for sdk v2's clean-break branding?

Once you green-light the tradeoffs above, phase 1 code starts immediately on this branch.

