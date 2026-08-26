"""typed verdicts — tri-state, not boolean."""
from __future__ import annotations
from dataclasses import dataclass
from typing import Literal, Any

VerdictDecision = Literal["approved", "denied", "escalate"]
RiskLevel = Literal["low", "medium", "high", "critical"]
ActionStatus = Literal["pending", "processing", "approved", "denied", "escalated", "forwarded", "forward_failed"]

@dataclass
class Verdict:
    action_id: str
    decision: VerdictDecision
    reason: str
    risk_level: RiskLevel
    status: ActionStatus | None = None
    policy_matched: str | None = None
    reasoning_chain: str | None = None
    confidence: float | None = None
    raw: dict[str, Any] | None = None

    @property
    def approved(self) -> bool:
        return self.decision == "approved"

    @property
    def denied(self) -> bool:
        return self.decision == "denied"

    @property
    def escalated(self) -> bool:
        return self.decision == "escalate"

@dataclass
class ForwardResult:
    forwarded: bool
    target_status: int | None = None
    target_response: str | None = None
    forward_error: str | None = None
    attempts: int | None = None
    raw: dict[str, Any] | None = None

@dataclass
class GuardOptions:
    auto_forward: bool = True
    on_denied: str = "raise"      # raise | return | callback
    on_escalate: str = "raise"    # raise | return | callback
    priority: int = 5
    timeout: float | None = None
