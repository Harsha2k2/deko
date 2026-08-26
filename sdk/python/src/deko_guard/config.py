"""config — env + explicit overrides, zero-config happy path."""
import os
from dataclasses import dataclass, field

def _env(name: str, default: str | None = None) -> str | None:
    v = os.getenv(name)
    return v if v is not None and v != "" else default

@dataclass
class DekoConfig:
    base_url: str = field(default_factory=lambda: _env("DEKO_URL", "http://localhost:8000") or "http://localhost:8000")
    api_key: str | None = field(default_factory=lambda: _env("DEKO_API_KEY"))
    jwt: str | None = field(default_factory=lambda: _env("DEKO_JWT"))
    auto_jwt: bool = True
    mode: str = field(default_factory=lambda: _env("DEKO_MODE", "blocking") or "blocking")
    timeout: float = 30.0
    max_retries: int = 2
    idempotency: bool = True
    wait: bool = True
    wait_timeout: int = 30

    def __post_init__(self):
        self.base_url = self.base_url.rstrip("/")
        if self.mode not in ("blocking", "polling"):
            self.mode = "blocking"

    @property
    def effective_timeout(self) -> float:
        return float(self.timeout)
