"""config — env + explicit overrides, zero-config happy path."""
import os
from dataclasses import dataclass

@dataclass
class DekoConfig:
    base_url: str = ""
    api_key: str | None = None
    jwt: str | None = None
    auto_jwt: bool = True
    mode: str = "blocking"
    timeout: float = 30.0
    max_retries: int = 2
    idempotency: bool = True
    wait: bool = True

    def __post_init__(self):
        if not self.base_url:
            self.base_url = os.getenv("DEKO_URL", "http://localhost:8000").rstrip("/")
        if self.api_key is None:
            self.api_key = os.getenv("DEKO_API_KEY")
        if self.jwt is None:
            self.jwt = os.getenv("DEKO_JWT")

