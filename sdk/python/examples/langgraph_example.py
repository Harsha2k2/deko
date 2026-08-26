"""langgraph + deko-guard — one-line adoption."""
from deko_guard import Deko

deko = Deko()  # reads DEKO_API_KEY / DEKO_URL

# wrap your tools
from deko_guard.adapters.langgraph import guard_tools

def refund(order_id: str, amount: float) -> str:
    """refund a customer"""
    return f"refunded {amount} for {order_id}"

guarded = guard_tools([refund], deko)

# or decorator style
@deko.guard(auto_forward=True)
def transfer(to: str, amount: float):
    """transfer money"""
    return {"to": to, "amount": amount}

# usage
# transfer(to="alice", amount=500)  # will raise DekoDeniedError if policy blocks
print("example loaded — set DEKO_API_KEY and run with a live deko server")
