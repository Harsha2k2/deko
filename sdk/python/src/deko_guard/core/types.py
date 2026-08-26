"""typed verdicts — tri-state, not boolean."""
from dataclasses import dataclass
from typing import Literal

VerdictDecision = Literal["approved", "denied", "escalate"]
RiskLevel = Literal["low", "medium", "high", "critical"]
ActionStatus = Literal["pending", "processing", "approved", "denied", "escalated", "forwarded", "forward_failed"]

@dataclass
class Verdict:
    action_id: str
    decision: VerdictDecision
    reason: str
    risk_level: RiskLevel
    policy_matched: str | None = None
    reasoning_chain: str | None = None
    confidence: float | None = None
    raw: dict | None = None

@dataclass
class ForwardResult:
    forwarded: bool
    target_status: int | None = None
    target_response: str | None = None
    forward_error: str | None = None
    attempts: int | None = None

@dataclass
class GuardOptions:
    auto_forward: bool = True
    on_denied: str = "raise"
    on_escalate: str = "raise"
    priority: int = 5
