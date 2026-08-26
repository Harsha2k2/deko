"""@guard / @aguard — the one-line adoption story."""
from __future__ import annotations
import functools
import inspect
import json
from typing import Any, Callable

from deko_guard.core.errors import DekoDeniedError, DekoEscalatedError
from deko_guard.core.idempotency import derive_key

def _derive_intent(fn: Callable, args: tuple, kwargs: dict, explicit: str | None = None) -> str:
    if explicit:
        return explicit
    doc = (fn.__doc__ or "").strip().split("\n")[0].strip()
    base = doc if doc else fn.__qualname__
    # include call args for policy matching
    try:
        sig = inspect.signature(fn)
        bound = sig.bind_partial(*args, **kwargs)
        bound.apply_defaults()
        args_str = ", ".join(f"{k}={v!r}" for k, v in bound.arguments.items())
    except Exception:
        args_str = ", ".join(repr(a) for a in args) + (", " if args and kwargs else "") + ", ".join(f"{k}={v!r}" for k, v in kwargs.items())
    return f"{base}({args_str})" if args_str else base

def guard(fn: Callable | None = None, *, deko=None, auto_forward: bool = True, on_denied: str = "raise",
          on_escalate: str = "raise", priority: int = 5, intent: str | None = None,
          target_url: str | None = None, target_method: str | None = None, **extra):
    def decorator(func: Callable):
        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any):
            if deko is None:
                raise RuntimeError("deko instance required: @deko.guard or guard(fn, deko=...)")
            # allow per-call override: refund(..., _deko_intent="custom")
            call_intent = kwargs.pop("_deko_intent", None) or _derive_intent(func, args, kwargs, intent)
            call_target_url = kwargs.pop("_deko_target_url", None) or target_url
            call_target_method = kwargs.pop("_deko_target_method", None) or target_method
            # payload is the tool's arguments as json
            try:
                payload_dict = {"args": args, "kwargs": kwargs}
                payload_str = json.dumps(payload_dict, default=str)
            except Exception:
                payload_str = str({"args": args, "kwargs": kwargs})
            idem = derive_key(func.__qualname__, args, kwargs) if deko.config.idempotency else None
            verdict = deko.check(
                intent=call_intent,
                payload=payload_str,
                target_url=call_target_url,
                target_method=call_target_method,
                priority=priority,
                idempotency_key=idem,
                **extra,
            )
            if verdict.decision == "denied":
                if on_denied == "raise":
                    raise DekoDeniedError(verdict)
                if on_denied == "return":
                    return {"_deko_verdict": verdict, "_deko_forwarded": None}
            if verdict.decision == "escalate":
                if on_escalate == "raise":
                    raise DekoEscalatedError(verdict)
                if on_escalate == "return":
                    return {"_deko_verdict": verdict, "_deko_forwarded": None}
            # approved
            if auto_forward and call_target_url:
                fwd = deko.forward(verdict.action_id)
                if not fwd.forwarded:
                    # forward_failed is retryable — surface as error
                    raise RuntimeError(f"forward failed: {fwd.forward_error}")
                # optionally call original function with forwarded response available
            return func(*args, **kwargs)
        return wrapper
    if fn is not None:
        return decorator(fn)
    return decorator

def aguard(fn: Callable | None = None, *, deko=None, auto_forward: bool = True, on_denied: str = "raise",
           on_escalate: str = "raise", priority: int = 5, intent: str | None = None,
           target_url: str | None = None, target_method: str | None = None, **extra):
    def decorator(func: Callable):
        @functools.wraps(func)
        async def wrapper(*args: Any, **kwargs: Any):
            if deko is None:
                raise RuntimeError("deko instance required")
            call_intent = kwargs.pop("_deko_intent", None) or _derive_intent(func, args, kwargs, intent)
            call_target_url = kwargs.pop("_deko_target_url", None) or target_url
            call_target_method = kwargs.pop("_deko_target_method", None) or target_method
            try:
                payload_dict = {"args": args, "kwargs": kwargs}
                payload_str = json.dumps(payload_dict, default=str)
            except Exception:
                payload_str = str({"args": args, "kwargs": kwargs})
            idem = derive_key(func.__qualname__, args, kwargs) if deko.config.idempotency else None
            verdict = await deko.acheck(
                intent=call_intent,
                payload=payload_str,
                target_url=call_target_url,
                target_method=call_target_method,
                priority=priority,
                idempotency_key=idem,
                **extra,
            )
            if verdict.decision == "denied" and on_denied == "raise":
                raise DekoDeniedError(verdict)
            if verdict.decision == "escalate" and on_escalate == "raise":
                raise DekoEscalatedError(verdict)
            if verdict.decision in ("denied", "escalate"):
                return {"_deko_verdict": verdict, "_deko_forwarded": None}
            if auto_forward and call_target_url:
                fwd = await deko.aforward(verdict.action_id)
                if not fwd.forwarded:
                    raise RuntimeError(f"forward failed: {fwd.forward_error}")
            return await func(*args, **kwargs) if inspect.iscoroutinefunction(func) else func(*args, **kwargs)
        return wrapper
    if fn is not None:
        return decorator(fn)
    return decorator
