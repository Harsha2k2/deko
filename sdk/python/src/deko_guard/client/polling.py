"""polling + ws wait — ws-first, polling fallback."""
from __future__ import annotations
import time
import random
from typing import Any

import httpx

from deko_guard.client.raw import DekoRawClient

def _parse_verdict_from_status(data: dict[str, Any]) -> dict[str, Any] | None:
    # status endpoint returns {status, verdict:{decision,reason,risk_level}} when ready
    if "verdict" in data and data["verdict"] is not None:
        return data["verdict"]
    # detail endpoint returns {status, verdict:{...}} as well
    if data.get("verdict") is not None:
        return data["verdict"]
    return None

def wait_for_verdict_sync(raw: DekoRawClient, action_id: str, timeout: float = 30.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    interval = 0.5
    # try ws first if available (optional)
    try:
        import asyncio
        # quick ws attempt via polling fallback for now — keep simple for v2a0
        pass
    except Exception:
        pass
    while True:
        data = raw.get_status(action_id)
        verdict = _parse_verdict_from_status(data)
        if verdict is not None:
            # also need action_id for typer
            verdict = dict(verdict)
            verdict["_action_id"] = data.get("action_id", action_id)
            verdict["_status"] = data.get("status")
            return verdict
        # check retry-after header via raw client? fallback to fixed
        retry_after = 1.0
        try:
            # raw.get_status doesn't expose headers; use interval
            pass
        except Exception:
            pass
        if time.monotonic() >= deadline:
            raise TimeoutError(f"verdict not ready within {timeout}s for {action_id}")
        sleep_for = min(retry_after + random.uniform(0, 0.5), 2.0)
        if time.monotonic() + sleep_for > deadline:
            sleep_for = max(0.1, deadline - time.monotonic())
        time.sleep(sleep_for)
        interval = min(interval * 1.2, 2.0)

async def wait_for_verdict_async(raw: DekoRawClient, action_id: str, timeout: float = 30.0) -> dict[str, Any]:
    import asyncio
    deadline = asyncio.get_event_loop().time() + timeout
    while True:
        data = await raw.aget_status(action_id)
        verdict = _parse_verdict_from_status(data)
        if verdict is not None:
            verdict = dict(verdict)
            verdict["_action_id"] = data.get("action_id", action_id)
            verdict["_status"] = data.get("status")
            return verdict
        if asyncio.get_event_loop().time() >= deadline:
            raise TimeoutError(f"verdict not ready within {timeout}s for {action_id}")
        await asyncio.sleep(0.5 + random.uniform(0, 0.3))
