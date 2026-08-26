import pytest
from deko_guard.core.types import Verdict
from deko_guard.core.errors import DekoDeniedError
from deko_guard.core.idempotency import derive_key

def test_derive_key_deterministic():
    assert derive_key("fn", (1,), {"a": 2}) == derive_key("fn", (1,), {"a": 2})
    assert derive_key("fn", (1,), {}) != derive_key("fn", (2,), {})

def test_verdict_properties():
    v = Verdict(action_id="1", decision="approved", reason="ok", risk_level="low")
    assert v.approved and not v.denied

def test_denied_error_carries_verdict():
    v = Verdict(action_id="1", decision="denied", reason="bad", risk_level="high")
    e = DekoDeniedError(v)
    assert e.verdict is v
