# deko architecture

living document. update it as reality changes. if code and this doc disagree,
one of them is wrong and both maintainers should know which.

## positioning

deko is the control plane for ai agent actions: every action an autonomous
agent wants to take against real systems passes through deko, gets evaluated
against deterministic policy plus llm risk analysis, and receives a provable
verdict: approve, deny, or escalate.

design north star: be the system of record. features can be mediocre at first;
the audit trail must never lie, and the gate must never silently open.

## principles (in priority order)

1. fail closed: any error in the decision path denies the action.
2. default deny: nothing is allowed unless something explicitly allows it.
3. tamper-evident history: audit entries are hash-chained; verdicts are
   immutable once written. corrections happen by new events, not edits.
4. boring infrastructure: single static binary, sqlite storage, no external
   services required for the pilot. scale-out paths are documented, not built.
5. api first: the ui consumes the same rest api that customers script against.

## context

```
                 ┌────────────────────────────────────────────────┐
 agents ────────▶│ deko (single rust binary)                      │
 (api key or     │                                                │
  jwt bearer)    │  edge      cors, body limits, per-ip rate limit│
                 │  identity  agent keys | jwt | admin sessions   │
                 │  decide    policy engine -> injection scan ->  │
                 │            llm provider chain -> verdict       │
                 │  act      guarded forwarding to target urls    │
                 │  record    hash-chained audit log, metrics     │
                 │                                                │
 admins ────────▶│  spa dashboard (/admin) + rest api (/api/*)    │
 (session cookie)└───────────────┬────────────────────────────────┘
                                 │
                          sqlite (wal mode)
                                 │
                     snapshots via `deko backup`
```

## components

### 1. edge layer

- cors: applied from config.allowlist origins (never wildcard in prod).
- body limits: request kb cap, screenshot mb cap.
- rate limit: sliding window per ip on agent routes; strict throttle on login.
- request id: uuid per request, echoed in logs and responses.

### 2. identity

agents (machines):
- api keys: random 256-bit, stored only as sha256(key:secret). multi-key per
  agent with labels and expiry.
- jwt bearer: hs256 tokens exchanged from keys via /auth/token; revocation is
  immediate because the agent row is re-checked per request.

users (humans): planned. single shared admin password is accepted debt for the
first pilot only; replacement is a users table (argon2id hashes), server-side
sessions (opaque token cookie), and four roles: owner, admin, analyst,
auditor. see roadmap.

### 3. decision pipeline (the core loop)

```
submit -> persist(pending)
       -> claim(processing)
         1. policy engine v2     deterministic, ordered, version-aware
         2. prompt-injection scan regex rules; critical = instant deny
         3. llm provider chain   primary -> fallbacks, timeout each,
                                 all providers failing = deny (fail closed)
         4. verdict tx           verdict + status + audit rows atomic
       -> webhook (hmac signed) + ws broadcast
forward -> egress guard -> relay to target, capture honest outcome
```

rules enforced on the pipeline itself:
- one evaluator implementation serves live decisions, dry-run, and the
  simulate endpoint. drift between those paths was a real bug class before.
- policy evaluation order is total: priority asc, then id asc. ties break
  deterministically.
- unknown rule types deny, not pass.
- processing claims are safe under concurrent replicas via compare-and-set
  status transitions (pending->processing claimed by one worker).

### 4. egress guard

every outbound http call deko makes on behalf of an action goes through one
choke point that enforces:
- scheme allowlist (http/https only)
- resolved ip must not be private, link-local, loopback, or cloud metadata
- redirects re-validated per hop (or disabled)
- response bodies capped and truncated before storage

applies to: action forwarding, webhooks, custom llm endpoints.

### 5. audit log

append-only table where each entry stores sha256(prev_hash || canonical
entry). the chain makes silent edits detectable; verification walks the chain
and reports the first broken link. exports are jsonl/csv with the chain head
embedded so recipients can check integrity offline.

### 6. storage

sqlite, wal mode, single writer. hot-path indexes: actions(status),
actions(agent_id, created_at), verdicts(action_id), audit_log(created_at).
`deko backup` produces a consistent online snapshot (wal checkpoint +
file copy) suitable for cron.

postgres is intentionally out for the pilot. the sql lives behind a thin data
layer so the port is mechanical when a customer actually needs it.

### 7. deployment

- docker: multi-stage cargo build, non-root, healthcheck hits /health/live
  using a tiny built-in client binary (no curl in the image).
- k8s/helm: single replica (sqlite constraint), liveness/readiness probes,
  secret-managed env. ha story = run two isolated environments behind an
  ingress split, or wait for postgres mode.

## explicit non-goals (pilot)

- multi-tenant orgs (schema stays ready, product does not expose)
- horizontal write scaling
- custom policy plugins/wasm
- streaming/partial verdicts

## roadmap

| phase | theme | items |
|-------|-------|-------|
| p0 | trust foundations | cors from config, sql interpolation removal, egress guard, attachment ownership, webhook hmac |
| p1 | provable core | audit hash chain + verify + exports, unified policy evaluator, provider fallback chain, honest forwarding states |
| p2 | humans | users + argon2 + sessions + rbac-lite, escalation queue polish |
| p3 | ship it | ci repair, docker healthcheck, helm chart, readme/docs truth pass |

## decision log

| date | decision | why |
|------|----------|-----|
| 2026-08 | drop half-done postgres scaffolding | two divergent dialects, neither tested; sqlite ships today, port is mechanical later |
| 2026-08 | unified auth middleware (key or bearer) | old stack required both, locking out every documented client |
| 2026-08 | single evaluator for live+simulate | simulate drift meant "tested" policies behaved differently in prod |
