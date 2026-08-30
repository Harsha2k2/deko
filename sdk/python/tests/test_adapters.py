import pytest
from deko_guard.adapters.openai import guard_openai_tools
from deko_guard.adapters.crewai import guard_crewai_tools, patch_crew
from deko_guard.adapters.langgraph import guard_tools as langgraph_guard_tools

class FakeDeko:
    def __init__(self):
        self.calls = []
        self.config = type("C", (), {"idempotency": False})()
    def guard(self, fn, **kw):
        def wrapped(*a, **kw2):
            self.calls.append(fn.__name__)
            return fn(*a, **kw2)
        wrapped.__name__ = fn.__name__
        return wrapped

def test_guard_openai_wraps():
    deko = FakeDeko()
    def my_fn(x): return x+1
    wrapped = guard_openai_tools([my_fn], deko)
    assert len(wrapped) == 1
    assert wrapped[0](5) == 6
    assert deko.calls == ["my_fn"]

def test_crewai_guard():
    deko = FakeDeko()
    def tool_a(x): return x*2
    guarded = guard_crewai_tools([tool_a], deko)
    assert guarded[0](3) == 6

def test_langgraph_guard_tools():
    deko = FakeDeko()
    def t1(x): return x
    out = langgraph_guard_tools([t1], deko)
    assert out[0](10) == 10

def test_patch_crew_agent_tools():
    deko = FakeDeko()
    class FakeAgent:
        def __init__(self):
            self.tools = [lambda x: x+1]
            self.tools[0].__name__ = "tool1"
    agent = FakeAgent()
    patch_crew(agent, deko)
    assert len(agent.tools) == 1
    assert agent.tools[0](5) == 6
