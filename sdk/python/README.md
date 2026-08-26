# deko-guard python sdk v2

`pip install deko-guard` — one-line guard for langgraph / crewai / openai / mcp.

```python
from deko_guard import Deko
deko = Deko()
@deko.guard(auto_forward=True)
def refund(order_id: str, amount: float): ...
```
