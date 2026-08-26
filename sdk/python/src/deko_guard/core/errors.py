"""fail-closed is typed."""
from deko_guard.core.types import Verdict

class DekoError(Exception): pass
class DekoAuthError(DekoError): pass
class DekoRateLimitedError(DekoError): pass
class DekoTimeoutError(DekoError): pass
class DekoValidationError(DekoError): pass

class DekoDeniedError(DekoError):
    def __init__(self, verdict: Verdict):
        super().__init__(f"denied: {verdict.reason}")
        self.verdict = verdict
        self.reason = verdict.reason
        self.risk_level = verdict.risk_level

class DekoEscalatedError(DekoError):
    def __init__(self, verdict: Verdict):
        super().__init__(f"escalated: {verdict.reason}")
        self.verdict = verdict
        self.reason = verdict.reason
