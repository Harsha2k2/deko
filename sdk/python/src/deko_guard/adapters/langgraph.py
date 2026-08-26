"""langgraph adapter — wrap ToolNode tools with one line."""
from __future__ import annotations
from typing import Any

def guard_tools(tools: list, deko, **guard_kwargs):
    """wrap a list of tool callables with deko guard."""
    return [deko.guard(t, **guard_kwargs) for t in tools]

def deko_node(tools: list, deko, **guard_kwargs):
    """return a langgraph-compatible node function that guards all tools.

    usage:
        from langgraph.prebuilt import ToolNode
        graph.add_node("tools", deko_node(tools, deko))
        # or
        graph.add_node("tools", ToolNode(guard_tools(tools, deko)))
    """
    guarded = guard_tools(tools, deko, **guard_kwargs)

    # map by name for dispatch
    by_name = {getattr(t, "__name__", getattr(t, "__qualname__", str(i))): t for i, t in enumerate(guarded)}
    # also try .name attribute (langchain tools)
    for t in guarded:
        for attr in ("name", "__name__"):
            if hasattr(t, attr):
                by_name[getattr(t, attr)] = t

    def node(state: dict[str, Any]):
        # langgraph state has last message with tool_calls
        messages = state.get("messages", [])
        if not messages:
            return {}
        last = messages[-1]
        tool_calls = getattr(last, "tool_calls", None) or last.get("tool_calls", []) if isinstance(last, dict) else []
        outputs = []
        for tc in tool_calls:
            name = tc.get("name") or tc.get("function", {}).get("name")
            args = tc.get("args") or tc.get("function", {}).get("arguments", {})
            if isinstance(args, str):
                import json as _json
                try:
                    args = _json.loads(args)
                except Exception:
                    args = {}
            fn = by_name.get(name)
            if fn is None:
                outputs.append({"tool_call_id": tc.get("id"), "content": f"unknown tool {name}"})
                continue
            try:
                # tools may be (kwargs) style
                if isinstance(args, dict):
                    content = fn(**args)
                else:
                    content = fn(*args) if isinstance(args, (list, tuple)) else fn(args)
                outputs.append({"tool_call_id": tc.get("id"), "content": str(content)})
            except Exception as e:
                # DekoDenied/Escalated bubble as tool error content, not graph crash
                outputs.append({"tool_call_id": tc.get("id"), "content": f"deko blocked: {e}"})
        return {"messages": outputs}

    return node
