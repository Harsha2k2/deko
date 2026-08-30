# sdk v2 — `deko-guard` 2.0.0

`pip install deko-guard` / `npm install deko-guard`

## python quick start (langgraph first)

```python
from deko_guard import Deko
deko = Deko()  # reads DEKO_URL=http://localhost:8000, DEKO_API_KEY

# decorator — one line
@deko.guard(auto_forward=True)
def refund(order_id: str, amount: float):
    """refund a customer"""
    return {"order_id": order_id, "amount": amount}

refund(order_id="ord_123", amount=500)  # raises DekoDeniedError if blocked
```

**langgraph:**
```python
from deko_guard.adapters.langgraph import guard_tools, deko_node
guarded = guard_tools([refund, transfer], deko)
graph.add_node("tools", deko_node(guarded, deko))
```

**openai / crewai:** `from deko_guard.adapters.openai import guard_openai_tools` / `crewai import guard_crewai_tools` — same pattern.

**mcp gate:** `deko-guard mcp --upstream "npx @modelcontextprotocol/server-filesystem /tmp"` — intercepts `tools/call`, approved → upstream, denied → `McpError -32600`.

**http proxy:** `deko-guard proxy --port 8080` + `export HTTP_PROXY=http://localhost:8080` — zero code change.

**admin plane (separate):** `from deko_guard.admin import DekoAdmin` — `pip install deko-guard[admin]` mental model, needs `DEKO_ADMIN_PASSWORD`.

## typescript

```ts
import { Deko } from "deko-guard";
const deko = new Deko({ apiKey: process.env.DEKO_API_KEY });
const safe = deko.guard(transferTool);
import { dekoMiddleware } from "deko-guard/vercel";
```

## architecture

See `../docs/sdk-architecture.md` for the 16-section plan covering core client (httpx sync+async, `?wait=true` one-roundtrip, ws→poll fallback), framework adapters, mcp gate, proxy, and 8 backend fixes.
