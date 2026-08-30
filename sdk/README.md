# deko-guard 2.0.0 — one-line guard for any agent

`pip install deko-guard` / `npm install deko-guard`

zero-config — `Deko()` reads `DEKO_URL` (default `http://localhost:8000`) and `DEKO_API_KEY` from env.

## python

```python
from deko_guard import Deko
deko = Deko()

@deko.guard  # sync, wait=true by default → one http round-trip
def refund(order_id: str, amount: float): return {"order_id": order_id, "amount": amount}

refund(order_id="ord_123", amount=500)  # ok → runs, blocked → raises DekoDeniedError
# async: @deko.aguard  + await refund(...)

# one-shot without decorator
v = deko.check(intent="delete all users")  # → Verdict(decision="denied", reason=..., risk_level=...)
```

adapters (same core, behind extras):
- `pip install deko-guard[langgraph]` → `from deko_guard.adapters.langgraph import guard_tools, deko_node`
- `pip install deko-guard[openai|crewai|mcp]` → `guard_openai_tools`, `guard_crewai_tools`, `deko-guard mcp --upstream "npx ..."`
- `deko-guard proxy --port 8080` + `HTTP_PROXY=http://localhost:8080` — zero code change, egress blocklist mirrored

admin plane separate (`from deko_guard.admin import DekoAdmin`, needs `DEKO_ADMIN_PASSWORD`):
```python
admin = DekoAdmin(password="...")
admin.create_policy(name="no-delete", rules=[{"type":"deny_keyword","keywords":["delete"]}])
```

## typescript

```ts
import { Deko } from "deko-guard";
const deko = new Deko({ apiKey: process.env.DEKO_API_KEY });
const safe = deko.guard(transferTool);
import { dekoMiddleware } from "deko-guard/vercel";
```

## live demo

`../deko-agents` has 5 ready agents (plain, langgraph, crewai, openai, mcp). after `deko` is up: `pip install -e ../deko/sdk/python && python deko_setup.py && python run_all.py`

see `../docs/sdk-architecture.md` for 16-section plan, backend `?wait` + `?token` fixes, and `shared/openapi.json`.

## publish

`2.0.0a0` tracks deko server `0.1.x` — `0.2.x` requires `deko >=0.1.8`.
