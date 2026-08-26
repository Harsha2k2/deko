"""raw http client — httpx sync+async, generated-style but hand-written for control."""
from __future__ import annotations
import time
from typing import Any

import httpx

from deko_guard.config import DekoConfig
from deko_guard.core.errors import DekoAuthError, DekoRateLimitedError, DekoValidationError, DekoError

class DekoRawClient:
    def __init__(self, config: DekoConfig):
        self.config = config
        self._jwt: str | None = config.jwt
        self._jwt_expires_at: float | None = None
        self._client = httpx.Client(base_url=config.base_url, timeout=config.timeout, follow_redirects=False)
        self._async_client = httpx.AsyncClient(base_url=config.base_url, timeout=config.timeout, follow_redirects=False)

    def _headers(self, use_jwt: bool = True) -> dict[str, str]:
        h: dict[str, str] = {"Content-Type": "application/json"}
        if use_jwt and self._jwt:
            h["Authorization"] = f"Bearer {self._jwt}"
        elif self.config.api_key:
            h["X-API-Key"] = self.config.api_key
        return h

    def _check(self, resp: httpx.Response) -> None:
        if resp.status_code == 401:
            raise DekoAuthError(resp.text)
        if resp.status_code == 429:
            retry = resp.headers.get("Retry-After")
            raise DekoRateLimitedError(retry_after=int(retry) if retry and retry.isdigit() else None)
        if resp.status_code == 422:
            raise DekoValidationError(resp.text)
        if resp.status_code >= 400:
            raise DekoError(f"{resp.status_code}: {resp.text}")

    # ---- sync ----
    def create_action(self, payload: dict[str, Any], wait: bool = False, timeout: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if wait:
            params["wait"] = "true"
            if timeout is not None:
                params["timeout"] = str(timeout)
        resp = self._client.post("/action", json=payload, headers=self._headers(), params=params or None)
        self._check(resp)
        return resp.json()

    def get_action(self, action_id: str) -> dict[str, Any]:
        resp = self._client.get(f"/action/{action_id}", headers=self._headers())
        self._check(resp)
        return resp.json()

    def get_status(self, action_id: str) -> dict[str, Any]:
        resp = self._client.get(f"/action/{action_id}/status", headers=self._headers())
        self._check(resp)
        return resp.json()

    def forward(self, action_id: str) -> dict[str, Any]:
        resp = self._client.post(f"/action/{action_id}/forward", headers=self._headers())
        # forward returns 200 even on forward_failed, 403/423 on denied/escalated
        if resp.status_code in (403, 423):
            # let facade map to typed errors via verdict, not http code alone
            try:
                return resp.json()
            except Exception:
                pass
        self._check(resp)
        return resp.json()

    def exchange_token(self) -> dict[str, Any]:
        # exchange uses api_key directly, no jwt header
        headers: dict[str, str] = {}
        if self.config.api_key:
            headers["X-API-Key"] = self.config.api_key
        resp = self._client.post("/auth/token", headers=headers)
        self._check(resp)
        data = resp.json()
        self._jwt = data.get("token")
        # naive expiry: now + expires_in - 300s margin
        if self._jwt:
            import time as _time
            self._jwt_expires_at = _time.time() + int(data.get("expires_in", 3600)) - 300
        return data

    def maybe_refresh_jwt(self) -> None:
        if not self.config.auto_jwt or not self.config.api_key:
            return
        import time as _time
        if self._jwt is None or (self._jwt_expires_at and _time.time() > self._jwt_expires_at):
            try:
                self.exchange_token()
            except Exception:
                pass

    # ---- async ----
    async def acreate_action(self, payload: dict[str, Any], wait: bool = False, timeout: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if wait:
            params["wait"] = "true"
            if timeout is not None:
                params["timeout"] = str(timeout)
        resp = await self._async_client.post("/action", json=payload, headers=self._headers(), params=params or None)
        self._check(resp)
        return resp.json()

    async def aget_action(self, action_id: str) -> dict[str, Any]:
        resp = await self._async_client.get(f"/action/{action_id}", headers=self._headers())
        self._check(resp)
        return resp.json()

    async def aget_status(self, action_id: str) -> dict[str, Any]:
        resp = await self._async_client.get(f"/action/{action_id}/status", headers=self._headers())
        self._check(resp)
        return resp.json()

    async def aforward(self, action_id: str) -> dict[str, Any]:
        resp = await self._async_client.post(f"/action/{action_id}/forward", headers=self._headers())
        if resp.status_code in (403, 423):
            try:
                return resp.json()
            except Exception:
                pass
        self._check(resp)
        return resp.json()

    async def aexchange_token(self) -> dict[str, Any]:
        headers: dict[str, str] = {}
        if self.config.api_key:
            headers["X-API-Key"] = self.config.api_key
        resp = await self._async_client.post("/auth/token", headers=headers)
        self._check(resp)
        data = resp.json()
        self._jwt = data.get("token")
        if self._jwt:
            import time as _time
            self._jwt_expires_at = _time.time() + int(data.get("expires_in", 3600)) - 300
        return data

    def close(self):
        self._client.close()

    async def aclose(self):
        await self._async_client.aclose()
