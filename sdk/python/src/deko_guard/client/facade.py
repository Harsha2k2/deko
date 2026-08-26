"""high-level facade — what users import as `Deko`."""
from __future__ import annotations
import json
from typing import Any

from deko_guard.config import DekoConfig
from deko_guard.client.raw import DekoRawClient
from deko_guard.client.polling import wait_for_verdict_sync, wait_for_verdict_async
from deko_guard.core.types import Verdict, ForwardResult
from deko_guard.core.errors import DekoDeniedError, DekoEscalatedError, DekoError

def _to_verdict(action_id: str, data: dict[str, Any], status: str | None = None) -> Verdict:
    # data is verdict dict possibly with _action_id / _status injected by polling
    return Verdict(
        action_id=data.get("_action_id", action_id),
        decision=data.get("decision", "denied"),  # type: ignore
        reason=data.get("reason", ""),
        risk_level=data.get("risk_level", "high"),  # type: ignore
        status=status,  # type: ignore
        policy_matched=data.get("policy_matched"),
        reasoning_chain=data.get("reasoning_chain"),
        confidence=data.get("confidence"),
        raw=data,
    )

class Deko:
    def __init__(self, config: DekoConfig | None = None, **overrides: Any):
        if config is None:
            self.config = DekoConfig(**overrides) if overrides else DekoConfig()
        else:
            self.config = config
            for k, v in overrides.items():
                if hasattr(self.config, k):
                    setattr(self.config, k, v)
        self.raw = DekoRawClient(self.config)

    def check(self, intent: str, *, payload: Any | None = None, target_url: str | None = None,
              target_method: str | None = None, priority: int = 5, idempotency_key: str | None = None,
              execute_at: str | None = None, metadata: dict | None = None,
              response_transform: Any | None = None, screenshot_base64: str | None = None,
              wait: bool | None = None, timeout: float | None = None) -> Verdict:
        """one-shot check — creates action and waits for verdict. does not forward."""
        if wait is None:
            wait = self.config.wait
        if timeout is None:
            timeout = self.config.wait_timeout if self.config.wait else self.config.timeout

        payload_str = json.dumps(payload) if payload is not None and not isinstance(payload, str) else payload
        body: dict[str, Any] = {"intent": intent, "priority": priority}
        if payload_str is not None: body["payload"] = payload_str
        if target_url: body["target_url"] = target_url
        if target_method: body["target_method"] = target_method
        if idempotency_key: body["idempotency_key"] = idempotency_key
        if execute_at: body["execute_at"] = execute_at
        if metadata is not None: body["metadata"] = metadata
        if response_transform is not None: body["response_transform"] = response_transform
        if screenshot_base64: body["screenshot_base64"] = screenshot_base64

        # auto exchange jwt if needed
        self.raw.maybe_refresh_jwt()

        if wait:
            # one-roundtrip path: POST /action?wait=true
            try:
                resp = self.raw.create_action(body, wait=True, timeout=int(timeout))
                # wait=true returns ActionDetailResponse with verdict embedded when ready
                if isinstance(resp, dict) and resp.get("verdict"):
                    v = resp["verdict"]
                    return _to_verdict(resp.get("id", ""), v, status=resp.get("status"))
                # if still pending (timeout), fall through to polling
                action_id = resp.get("id")
                if not action_id:
                    raise DekoError(f"unexpected response: {resp}")
                # polling fallback
                data = wait_for_verdict_sync(self.raw, action_id, timeout=timeout)
                return _to_verdict(action_id, data, status=data.get("_status"))
            except Exception as e:
                # if wait endpoint not yet deployed, fallback to polling
                if "404" in str(e) or "not found" in str(e).lower():
                    pass
                else:
                    # try polling path anyway
                    pass

        # classic two-step
        resp = self.raw.create_action(body, wait=False)
        action_id = resp.get("id")
        if not action_id:
            raise DekoError(f"create_action missing id: {resp}")
        data = wait_for_verdict_sync(self.raw, action_id, timeout=timeout)
        return _to_verdict(action_id, data, status=data.get("_status"))

    async def acheck(self, intent: str, **kwargs: Any) -> Verdict:
        """async version of check."""
        # for now delegate to sync via anyio? keep simple: use async raw directly
        payload = kwargs.get("payload")
        import json as _json
        payload_str = _json.dumps(payload) if payload is not None and not isinstance(payload, str) else payload
        body: dict[str, Any] = {"intent": kwargs.get("intent", intent), "priority": kwargs.get("priority", 5)}
        # rebuild body similar to check but async polling
        body = {"intent": intent, "priority": kwargs.get("priority", 5)}
        if payload_str is not None: body["payload"] = payload_str
        if kwargs.get("target_url"): body["target_url"] = kwargs["target_url"]
        if kwargs.get("target_method"): body["target_method"] = kwargs["target_method"]
        if kwargs.get("idempotency_key"): body["idempotency_key"] = kwargs["idempotency_key"]
        wait = kwargs.get("wait", self.config.wait)
        timeout = kwargs.get("timeout", self.config.wait_timeout if self.config.wait else self.config.timeout)
        resp = await self.raw.acreate_action(body, wait=bool(wait), timeout=int(timeout) if wait else None)
        if isinstance(resp, dict) and resp.get("verdict"):
            v = resp["verdict"]
            return _to_verdict(resp.get("id", ""), v, status=resp.get("status"))
        action_id = resp.get("id")
        data = await wait_for_verdict_async(self.raw, action_id, timeout=float(timeout))
        return _to_verdict(action_id, data, status=data.get("_status"))

    def forward(self, action_id: str) -> ForwardResult:
        data = self.raw.forward(action_id)
        return ForwardResult(
            forwarded=bool(data.get("forwarded")),
            target_status=data.get("target_status"),
            target_response=data.get("target_response"),
            forward_error=data.get("forward_error"),
            attempts=data.get("forward_attempts"),
            raw=data,
        )

    async def aforward(self, action_id: str) -> ForwardResult:
        data = await self.raw.aforward(action_id)
        return ForwardResult(
            forwarded=bool(data.get("forwarded")),
            target_status=data.get("target_status"),
            target_response=data.get("target_response"),
            forward_error=data.get("forward_error"),
            attempts=data.get("forward_attempts"),
            raw=data,
        )

    # syntactic sugar: deko(intent=...) -> check
    def __call__(self, intent: str, **kwargs: Any) -> Verdict:
        return self.check(intent, **kwargs)

    def close(self):
        self.raw.close()

    async def aclose(self):
        await self.raw.aclose()

    # decorator factories — thin wrappers over core/guard
    def guard(self, func=None, *, auto_forward: bool = True, on_denied: str = "raise", on_escalate: str = "raise", priority: int = 5, **guard_kwargs):
        from deko_guard.core.guard import guard as _guard
        if func is not None:
            return _guard(func, deko=self, auto_forward=auto_forward, on_denied=on_denied, on_escalate=on_escalate, priority=priority, **guard_kwargs)
        def decorator(fn):
            return _guard(fn, deko=self, auto_forward=auto_forward, on_denied=on_denied, on_escalate=on_escalate, priority=priority, **guard_kwargs)
        return decorator

    def aguard(self, func=None, *, auto_forward: bool = True, on_denied: str = "raise", on_escalate: str = "raise", priority: int = 5, **guard_kwargs):
        from deko_guard.core.guard import aguard as _aguard
        if func is not None:
            return _aguard(func, deko=self, auto_forward=auto_forward, on_denied=on_denied, on_escalate=on_escalate, priority=priority, **guard_kwargs)
        def decorator(fn):
            return _aguard(fn, deko=self, auto_forward=auto_forward, on_denied=on_denied, on_escalate=on_escalate, priority=priority, **guard_kwargs)
        return decorator
