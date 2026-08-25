# Deko - AI Agent Action Watchdog

**Deko** is a control plane for autonomous AI agents. It intercepts every action an
agent wants to perform against real systems, evaluates it against deterministic
policy plus AI-powered risk analysis, and issues a provable verdict:
**approve**, **deny**, or **escalate for human review**.

Think of it as a firewall plus flight recorder for AI agents: no action reaches
your systems without passing through Deko, and every decision is tamper-evidently
recorded.

---

## Core Principles

| Principle | What It Means |
|---|---|
| **Default-Deny** | Every action is blocked unless explicitly approved |
| **Fail-Closed** | Any error in the decision path denies the action — never silently approves |
| **Tamper-Evident Audit** | Every decision is logged into a sha256 hash chain; edits break the chain and are detectable via `GET /admin/audit/verify` |
| **Human Override** | A human can override any denial; the override is itself chained into the audit log |

Design and architecture decisions live in [docs/architecture.md](docs/architecture.md).

## How It Works

```
AI Agent ──submit action──▶ Deko ──▶ Policy Engine v2 ──▶ Injection Scan ──▶ LLM Chain ──▶ Verdict
                                    (ordered rules,       (regex rules,      (provider    Approve /
                                     versioned)            critical = deny)   fallbacks)   Deny / Escalate
                                                                              │
                                                                  ┌───────────┼───────────┐
                                                                  ▼           ▼           ▼
                                                             Forward      Block      Notify Admin
                                                             (egress      + honest   (hmac-signed
                                                              guarded)     failure     webhook)
```

### The Action Lifecycle

1. **Register an agent** — admin registers each agent; Deko returns an API key
   (hashed at rest). Multiple keys per agent supported, with expiry.
2. **Agent submits an action** — `POST /action` with intent, optional payload,
   optional screenshot. Dangerous target URLs are rejected *at submit time* by
   the egress guard.
3. **Deko evaluates** — deterministic ordered policy rules → prompt-injection
   scan → LLM analysis through the provider chain (first success wins;
   all providers failing = fail-closed deny).
4. **Verdict recorded** — verdict + status + audit entries written atomically,
   chained into the audit hash chain.
5. **Agent polls** — `GET /action/{id}/status` until verdict is ready.
6. **Forwarding (if approved)** — `POST /action/{id}/forward` relays the request
   to its target through the egress guard: private/link-local/metadata IPs
   blocked, redirects re-validated per hop. Delivery failures set an honest
   `forward_failed` status so agents can retry — Deko never reports success
   that didn't happen.
7. **Admin override** — denied/escalated actions can be overridden by an
   authenticated admin; overrides are audited.

---

## Quick Start

### Prerequisites

- Rust 1.75+ (`rustup` recommended)
- At least one LLM API key (Gemini, OpenAI, Anthropic, Ollama, Azure, Bedrock, or custom)

### Run Locally

```bash
git clone git@github.com:Harsha2k2/deko.git
cd deko

cp .env.example .env
# edit .env -- at minimum: admin password, api key secret, one llm key

cargo build --release
cargo run
```

Server starts on `http://localhost:8000`.

### Run with Docker

```bash
docker compose up -d
```

The image ships a built-in healthcheck client (no curl/wget needed in-container).

### Verify

```bash
curl http://localhost:8000/health
curl http://127.0.0.1:8000/admin/login -X POST \
  -d 'password=your-admin-password' -c cookies.txt
curl http://localhost:8000/api/admin/dashboard -b cookies.txt
```

---

## First-Time Setup

### Step 1: Authenticate as Admin

```bash
# interactive/session use: exchange password for a server-side session cookie
curl -X POST http://localhost:8000/admin/login \
  -d 'password=your-admin-password' -c cookies.txt

# scripted use: send X-Admin-Password on every admin call
ADMIN_H='X-Admin-Password: your-admin-password'
```

Sessions are opaque 256-bit tokens stored hashed server-side; logout deletes
them. The raw password never travels as a cookie.

### Step 2: Register an Agent

```bash
curl -X POST http://localhost:8000/admin/agents/register \
  -H "$ADMIN_H" -H "Content-Type: application/json" \
  -d '{"name": "my-agent"}'
```

Response contains the agent `id` and `api_key` — save the key; it is stored only
as a hash.

### Step 3: Submit an Action

```bash
curl -X POST http://localhost:8000/action \
  -H "X-API-Key: <agent-key>" -H "Content-Type: application/json" \
  -d '{
    "intent": "Transfer $500 to account 12345",
    "payload": "{\"amount\": 500}",
    "target_url": "https://bank.example.com/api/transfer",
    "target_method": "POST"
  }'
```

Agents may also exchange their API key for a short-lived JWT via
`POST /auth/token`, then call action endpoints with `Authorization: Bearer`.
Both credentials work on every agent endpoint.

### Step 4: Poll, Then Forward

```bash
curl http://localhost:8000/action/<id>/status -H "X-API-Key: <agent-key>"
curl -X POST http://localhost:8000/action/<id>/forward -H "X-API-Key: <agent-key>"
```

A successful forward returns `{"forwarded": true, ...}`; an undeliverable
request returns `{"forwarded": false, "forward_error": ...}` and marks the
action `forward_failed`.

---

## API Reference

### Agent Endpoints (API key or JWT)

| Method | Endpoint | Description |
|---|---|---|
| `POST` | `/action` | Submit an action for review |
| `POST` | `/actions/batch` | Submit up to 50 actions |
| `GET` | `/action/{id}` | Full action details including verdict |
| `GET` | `/action/{id}/status` | Poll for verdict (`Retry-After` when pending) |
| `POST` | `/action/{id}/forward` | Relay an approved action to its target |
| `GET` | `/actions` | List own actions (`?status=` filter, pagination) |
| `POST` | `/action/{id}/attachments` | Upload attachment (multipart, 10 MB cap) |
| `GET` | `/action/{id}/attachments` | List attachments |
| `GET` | `/action/{id}/attachments/{aid}` | Download attachment (owner-checked) |
| `GET` | `/action/{id}/ws` | WebSocket for live status |
| `POST` | `/auth/token` | Exchange API key for JWT |

### Admin Endpoints (session cookie or `X-Admin-Password`)

| Method | Endpoint | Description |
|---|---|---|
| `POST` | `/admin/login` / `GET|POST /admin/logout` | Session lifecycle |
| `POST` | `/admin/agents/register` · `/revoke` · `/rotate-key` | Agent management |
| `POST` | `/admin/agents/create-api-key` · `/list-api-keys` | Multi-key support |
| `GET|POST|PUT|DELETE` | `/admin/policies[...]` | Policy CRUD |
| `POST` | `/admin/policies/test` · `/simulate` | Dry-run policies (same evaluator as production) |
| `GET` | `/admin/audit/export` · `/admin/audit/search` | Audit access |
| `GET` | `/admin/audit/verify` | Walk the audit hash chain; report first broken link |

### Dashboard JSON API (same auth)

`/api/admin/dashboard`, `/api/admin/actions[/...]`, `/api/admin/agents`,
`/api/admin/verdicts[...]`, `/api/admin/policies`, `/api/admin/audit`,
`/api/admin/ws`.

### Health & Observability

| Endpoint | Description |
|---|---|
| `/health` · `/health/live` · `/health/ready` | Full health + k8s probes |
| `/metrics` | JSON metrics |
| `/metrics/prometheus` | Prometheus text format |
| `/docs` | Swagger UI |

---

## Policy Engine

Policies are ordered rule arrays evaluated deterministically: policies by age,
rules by their optional `priority` field (lower first), then declaration order.
One implementation serves production decisions, dry-run mode
(`DEKO_POLICY_DRY_RUN`), and the test/simulate endpoints — what you simulate is
exactly what enforces.

| Rule Type | Behavior |
|---|---|
| `deny_keyword` | Deny if intent contains any keyword (critical) |
| `regex_deny` | Deny if intent+payload matches pattern (critical) |
| `max_amount` | Deny if payload `$.amount` exceeds max |
| `require_approval` | Flag matching HTTP methods for review |
| `risk_flag` | Medium-risk annotation on keyword match |
| `url_allowlist` / `url_blocklist` | Target URL constraints |
| `time_window` | Restrict to UTC hour window / weekdays |
| `ip_allowlist` · `geofence` | Metadata-based source constraints |
| `rate_limit` · `concurrency_limit` · `budget_limit` · `histogram_trend` | Database-backed behavioral limits |
| `and` / `or` | Composites over sub-rules |
| *unknown type* | **Denies (fail-closed)** — typos stop traffic instead of passing silently |

```json
[
  {"type": "deny_keyword", "keywords": ["delete all", "drop table"], "priority": 1},
  {"type": "max_amount", "max": 10000},
  {"type": "url_blocklist", "patterns": ["known-bad.example"]}
]
```

---

## Security Model

### Egress Guard

Every outbound URL deko contacts on behalf of an action passes one choke point:

- schemes limited to http/https; userinfo tricks rejected
- literal and DNS-resolved addresses checked against blocklists: loopback,
  rfc1918, link-local (incl. cloud metadata `169.254.169.254`), CGNAT, ULA,
  multicast, documentation ranges, v4-mapped v6 bypasses
- redirects followed manually (max 3 hops) with full re-validation per hop
- applies to action forwarding *and* webhook delivery
- known residual risk: classic DNS rebinding TOCTOU (documented in
  `src/services/egress.rs`; full mitigation requires connection pinning)

### Authentication

- **Agents**: API keys (sha256-hashed with server secret, multi-key, expiry) or
  HS256 JWTs exchanged from keys. Agent rows are re-checked per request, so
  revocation is immediate.
- **Admins**: server-side sessions (opaque token cookie; only its hash stored)
  or the bootstrap `X-Admin-Password` header compared in constant time.
  Per-IP sliding throttle on login. Forged or legacy cookies grant nothing.

### Webhooks

Denied/escalated verdict notifications are signed:
`X-Deko-Signature: t=<unix>,v1=<hex hmac-sha256(timestamp.payload)>`.
Receivers reject timestamps older than ~5 minutes to block replay — same
verification scheme as GitHub/Stripe.

### Input Handling

Parameterized SQL everywhere (including JSON paths inside policy rules);
request-body and screenshot size caps; HTML-escaping of intent fields;
uploads stored outside every served path behind ownership checks.

### Fail-Closed Design

LLM unreachable after the full provider chain → deny. Processing error → deny.
Audit-chain backfill failure at boot → process refuses to start.

---

## Configuration

Key environment variables (see `.env.example` for the full list):

| Variable | Default | Description |
|---|---|---|
| `DEKO_PORT` | `8000` | Listen port |
| `DEKO_ENV` | `dev` | `dev` enables permissive CORS defaults; `prod` requires explicit origins |
| `DEKO_ADMIN_PASSWORD` | *(required)* | Bootstrap admin password (min 8 chars) |
| `DEKO_API_KEY_SECRET` | *(required)* | Secret mixed into API-key hashes |
| `DEKO_ALLOWED_ORIGINS` | dev default | CORS allowlist (enforced, not decorative) |
| `DEKO_RATE_LIMIT_PER_MINUTE` | `120` dev / `30` prod | Per-IP agent-route limit |
| `LLM_DEFAULT_PROVIDER` | `gemini` | First provider tried |
| `<PROVIDER>_API_KEY` etc. | - | Any configured provider joins the fallback chain |
| `DEKO_WEBHOOK_URL` · `DEKO_WEBHOOK_SECRET` | - | Verdict notifications + signing secret |
| `DEKO_POLICY_DRY_RUN` | unset | Log would-be denials without enforcing |
| `DEKO_BACKUP_DIR` | unset | Pre-migration sqlite file backups |

## Production Checklist

- [ ] Strong `DEKO_ADMIN_PASSWORD` and random 32+ char `DEKO_API_KEY_SECRET`
- [ ] `DEKO_ENV=prod` (structured logs, strict CORS required)
- [ ] At least two configured LLM providers so the fallback chain has depth
- [ ] `DEKO_WEBHOOK_URL` + `DEKO_WEBHOOK_SECRET` for escalation alerts
- [ ] Volume mount `/app/data` (sqlite database lives there); schedule
      `sqlite3 .backup` or file snapshots
- [ ] TLS termination at your ingress; IP allowlisting belongs there too
- [ ] Monitor `/metrics/prometheus`; alert on `/health` failures
- [ ] Verify audit integrity periodically: `GET /admin/audit/verify`

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (axum, sqlx, tokio) |
| Storage | SQLite (WAL mode) |
| Crypto | rustls, HMAC-SHA256 webhook signatures, sha256 audit chain |
| Templates/UI | Askama pages + React admin SPA |
| API Docs | Utoipa + Swagger UI |
| Containerization | Docker (multi-stage, non-root, built-in healthcheck) |
| CI | GitHub Actions: fmt, clippy `-D warnings`, tests, cargo-audit, trivy |

## What This Build Is Not (Yet)

Honest boundaries of the pilot appliance, from
[docs/architecture.md](docs/architecture.md):

- single-node sqlite (postgres port is a planned data-layer task, not a flag)
- no multi-user RBAC yet (single bootstrap admin; users/roles on the roadmap)
- horizontal write scaling out of scope

## License

MIT
