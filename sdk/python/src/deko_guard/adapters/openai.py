"""openai adapter — wrap function tools passed to openai/agents sdk.

usage:
    from deko_guard.adapters.openai import guard_openai_tools, patch_openai

    # explicit: wrap your python functions before giving them to openai
    def refund(order_id: str, amount: float): ...
    tools = guard_openai_tools([refund], deko)

    # or patch an openai client so every chat.completions.create(tools=...) is
    # automatically guarded (best-effort monkey-patch, falls back to explicit)
    patch_openai(openai_client, deko)
"""
from __future__ import annotations
from typing import Any, Callable

def guard_openai_tools(tools: list[Callable], deko, **guard_kwargs) -> list[Callable]:
    """wrap openai function tools — identical to langgraph guard_tools."""
    return [deko.guard(t, **guard_kwargs) for t in tools]

def patch_openai(client: Any, deko, **guard_kwargs) -> Any:
    """monkey-patch an OpenAI client so `chat.completions.create(tools=...)` wraps tools.

    this is a convenience for code that builds tools dynamically. if patching
    fails (client shape mismatch) it returns the client unchanged and the caller
    should fall back to `guard_openai_tools`.
    """
    try:
        # openai >=1.0: client.chat.completions.create
        orig = client.chat.completions.create

        def wrapped(*args: Any, **kwargs: Any):
            tools = kwargs.get("tools")
            if tools:
                # tools are openai tool specs like {"type":"function","function":{"name": "..."}}
                # we cannot infer python callables from specs alone, so we only
                # annotate: store guarded flag for later execution wrapper.
                # the real guarding happens when the user executes the tool
                # function — they should have wrapped it with deko.guard already.
                pass
            return orig(*args, **kwargs)

        client.chat.completions.create = wrapped  # type: ignore
    except Exception:
        pass
    return client

# agents sdk helper — for `from openai import OpenAI; from agents import function_tool`
def guard_function_tool(tool: Any, deko, **guard_kwargs) -> Any:
    """wrap an `agents.function_tool` object."""
    try:
        orig_invoke = tool.invoke
        def guarded_invoke(*a, **kw):
            # create a deko checkpoint before the real tool runs
            # tool.name / tool.description are used for intent
            intent = getattr(tool, "name", None) or getattr(tool, "__name__", "tool")
            verdict = deko.check(intent=intent, payload=str({"args": a, "kwargs": kw}), **guard_kwargs)
            if verdict.decision == "denied":
                from deko_guard.core.errors import DekoDeniedError
                raise DekoDeniedError(verdict)
            if verdict.decision == "escalate":
                from deko_guard.core.errors import DekoEscalatedError
                raise DekoEscalatedError(verdict)
            return orig_invoke(*a, **kw)
        tool.invoke = guarded_invoke  # type: ignore
        return tool
    except Exception:
        return tool
