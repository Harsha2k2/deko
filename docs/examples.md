# Deko Integration Examples

## Python SDK Example

```python
import requests
import time

API_URL = "http://localhost:8000"
API_KEY = "<your-agent-key>"

def submit_action(intent, payload=None, target_url=None, target_method="POST"):
    resp = requests.post(
        f"{API_URL}/action",
        headers={"X-API-Key": API_KEY, "Content-Type": "application/json"},
        json={
            "intent": intent,
            "payload": payload,
            "target_url": target_url,
            "target_method": target_method,
        }
    )
    result = resp.json()
    action_id = result["id"]

    while True:
        status = requests.get(
            f"{API_URL}/action/{action_id}/status",
            headers={"X-API-Key": API_KEY}
        ).json()
        if status["status"] != "pending":
            return status
        time.sleep(2)

# Example: Trading bot
verdict = submit_action(
    intent="Buy 100 shares of AAPL at market price",
    payload='{"symbol": "AAPL", "quantity": 100, "order_type": "market"}',
    target_url="https://broker.example.com/api/trade"
)
print(f"Verdict: {verdict['verdict']['decision']} - {verdict['verdict']['reason']}")
```

## LangChain Integration

```python
from langchain.tools import tool
import requests

@tool
def deko_guard(action_intent: str, payload: str = None) -> str:
    """Submit an action to Deko for security review before execution."""
    resp = requests.post(
        "http://localhost:8000/action",
        headers={"X-API-Key": "<agent-key>"},
        json={"intent": action_intent, "payload": payload}
    )
    return resp.text
```

## AutoGen Integration

```python
import autogen
import requests

def deko_tool(action_intent: str) -> dict:
    response = requests.post(
        "http://localhost:8000/action",
        headers={"X-API-Key": "<agent-key>"},
        json={"intent": action_intent}
    )
    return {"action_id": response.json()["id"], "status": response.json()["status"]}
```

## Trading Bot Example

```python
# A simple trading agent that checks with Deko before each trade
import requests
import time

class TradingBot:
    def __init__(self, deko_api_key: str):
        self.api_key = deko_api_key
        self.base_url = "http://localhost:8000"

    def execute_trade(self, symbol: str, quantity: int, side: str):
        intent = f"{side} {quantity} shares of {symbol}"
        resp = requests.post(
            f"{self.base_url}/action",
            headers={"X-API-Key": self.api_key},
            json={
                "intent": intent,
                "payload": f'{{"symbol": "{symbol}", "quantity": {quantity}}}',
                "target_url": "https://paper-broker.example.com/trade",
                "target_method": "POST",
            }
        )
        action_id = resp.json()["id"]

        while True:
            status = requests.get(
                f"{self.base_url}/action/{action_id}/status",
                headers={"X-API-Key": self.api_key}
            ).json()
            if status["status"] != "pending":
                if status["status"] == "approved":
                    forward = requests.post(
                        f"{self.base_url}/action/{action_id}/forward",
                        headers={"X-API-Key": self.api_key}
                    )
                    return forward.json()
                return {"error": "Trade denied", "reason": status["verdict"]["reason"]}
            time.sleep(1)

# Usage
bot = TradingBot(api_key="your-key")
result = bot.execute_trade("AAPL", 10, "Buy")
print(result)
```
