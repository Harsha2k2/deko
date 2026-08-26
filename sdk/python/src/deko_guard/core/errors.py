"""fail-closed is typed."""
from __future__ import annotations
from deko_guard.core.types import Verdict

class DekoError(Exception):
    """base for all deko-guard errors."""
    pass

class DekoAuthError(DekoError):
    pass

class DekoRateLimitedError(DekoError):
    def __init__(self, message: str = "rate limited", retry_after: int | None = None):
        super().__init__(message)
        self.retry_after = retry_after

class DekoTimeoutError(DekoError):
    pass

class DekoValidationError(DekoError):
    pass

class DekoDeniedError(DekoError):
    def __init__(self, verdict: Verdict):
        super().__init__(f"denied: {verdict.reason} (risk={verdict.risk_level})")
        self.verdict = verdict
        self.reason = verdict.reason
        self.risk_level = verdict.risk_level
        self.policy_matched = verdict.policy_matched

class DekoEscalatedError(DekoError):
    def __init__(self, verdict: Verdict):
        super().__init__(f"escalated: {verdict.reason}")
        self.verdict = verdict
        self.reason = verdict.reason
        self.risk_level = verdict.risk_level
