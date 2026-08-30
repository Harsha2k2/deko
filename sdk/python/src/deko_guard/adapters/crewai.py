"""crewai adapter — one-line guard for crewai @tool functions and CrewAgent tools.

usage:
    from deko_guard.adapters.crewai import guard_crewai_tools, patch_crew

    @tool
    def refund(order_id: str, amount: float): ...

    guarded = guard_crewai_tools([refund], deko)

    # or patch a Crew / Agent in place
    crew = Crew(agents=[...], tasks=[...])
    patch_crew(crew, deko)
"""
from __future__ import annotations
from typing import Any

def guard_crewai_tools(tools: list, deko, **guard_kwargs) -> list:
    """wrap crewai tool callables — same core as langgraph."""
    return [deko.guard(t, **guard_kwargs) for t in tools]

def patch_crew(crew: Any, deko, **guard_kwargs) -> Any:
    """patch a CrewAgent or Crew to guard all its tools."""
    try:
        # crewai Crew has .agents; Agent has .tools
        agents = getattr(crew, "agents", None)
        if agents is not None:
            for agent in agents:
                tools = getattr(agent, "tools", None)
                if tools:
                    agent.tools = guard_crewai_tools(list(tools), deko, **guard_kwargs)
            return crew
        # maybe it's an Agent directly
        tools = getattr(crew, "tools", None)
        if tools is not None:
            crew.tools = guard_crewai_tools(list(tools), deko, **guard_kwargs)
    except Exception:
        pass
    return crew

def guard_tool(tool: Any, deko, **guard_kwargs) -> Any:
    """wrap a single crewai tool object (has .func or is callable)."""
    # crewai tools are often objects with .func
    func = getattr(tool, "func", None)
    if callable(func):
        try:
            tool.func = deko.guard(func, **guard_kwargs)  # type: ignore
            return tool
        except Exception:
            pass
    if callable(tool):
        return deko.guard(tool, **guard_kwargs)  # type: ignore
    return tool
