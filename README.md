# Deko — AI Agent Action Watchdog

control plane + flight recorder for autonomous agents. every action an agent wants to do against real systems must pass deko, get a verdict, and leave a tamper-evident record.

**principles:** default-deny, fail-closed, sha256 hash-chained audit, human override is itself audited.

see `docs/architecture.md` for decisions.

## how it works

```
agent → POST /action → deko: policy (ordered) → injection scan → llm chain (fallbacks) → verdict
                                    → forward (egress-guarded) / block / webhook (hmac)
```

1. admin registers agent → gets `api_key` (hashed at rest)
2. agent `POST /action` (intent, payload, target_url) — dangerous urls rejected at submit
3. deko evaluates → writes `verdict` + `audit` atomically → `GET /action/{id}/status` to poll, or `POST /action?wait=true` for one round-trip
4. if `approved` → `POST /action/{id}/forward` relays, `forward_failed` on delivery failure
5. admin can `POST /api/admin/actions/{id}/override` — override is chained

## quick start

```bash
git clone git@github.com:Harsha2k2/deko.git && cd deko
cp .env.example .env  # set DEKO_ADMIN_PASSWORD, DEKO_API_KEY_SECRET, GEMINI_API_KEY
cargo build --release && cargo run  # :8000
# or: docker compose up -d
curl http://localhost:8000/health
curl -X POST http://localhost:8000/admin/login -d 'password=...' -c cookies.txt
open http://localhost:8000/admin  # login with same password
```

register + submit:
```bash
ADMIN_H='X-Admin-Password: ...'
curl -X POST http://localhost:8000/admin/agents/register -H "$ADMIN_H" -d '{"name":"my-agent"}' # → {id, api_key}
curl -X POST http://localhost:8000/action -H "X-API-Key: <key>" -d '{"intent":"refund $500","payload":"{\"amount\":500}"}'
curl http://localhost:8000/action/<id>/status -H "X-API-Key: <key>"
```

## sdk

`pip install deko-guard` / `npm install deko-guard` — see `sdk/README.md` and `docs/sdk-architecture.md`.

```python
from deko_guard import Deko
deko = Deko()  # DEKO_URL + DEKO_API_KEY from env
@deko.guard
def refund(order_id: str, amount: float): return {"order_id": order_id}
refund(order_id="ord_123", amount=500)  # raises DekoDeniedError if blocked
```

## api (auth: `X-API-Key` or `Bearer JWT` for agents, `deko_session` or `X-Admin-Password` for admin)

agents: `POST /action` (use `?wait=true`), `GET /action/{id}`, `GET /action/{id}/status`, `POST /action/{id}/forward`, `GET /actions`, `POST /auth/token`, `GET /action/{id}/ws?token=`
admin: `POST /admin/login|logout`, `/admin/agents/*`, `/admin/policies`, `POST /admin/policies/test|simulate`, `GET /admin/audit/verify`
ui json: `/api/admin/*`, health: `/health`, `/metrics`, docs: `/docs`

## policies

ordered `rules` array, single evaluator for prod/simulate. `deny_keyword`, `regex_deny`, `max_amount`, `url_allowlist|blocklist`, `time_window`, `rate_limit` etc. unknown type → deny.

## security

egress guard (private/metadata/cgnat block, 3-hop re-validate), hmac webhooks, parameterized sql, `forward_failed` honest, `deko_session` HttpOnly.

## config

`DEKO_PORT=8000`, `DEKO_ENV=prod` needs explicit origins, `DEKO_ADMIN_PASSWORD` (8+), `DEKO_API_KEY_SECRET` (16+), `LLM_DEFAULT_PROVIDER=gemini`, `GEMINI_API_KEY`, `DEKO_WEBHOOK_URL`.

## boundaries

single-node sqlite (postgres later), single admin password (users/roles next), no horizontal write scaling yet.

license: MIT
