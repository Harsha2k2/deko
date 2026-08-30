"""DekoAdmin — separate admin plane client (requires DEKO_ADMIN_PASSWORD).

not for agents — for ci / policy-as-code. keep admin password out of agent env.

    from deko_guard.admin import DekoAdmin
    admin = DekoAdmin(password="...")  # or DEKO_ADMIN_PASSWORD env
    admin.create_policy(name="no-delete", rules=[{"type":"deny_keyword","keywords":["delete"]}])
"""
from __future__ import annotations
import os
from typing import Any

import httpx

class DekoAdmin:
    def __init__(self, base_url: str | None = None, password: str | None = None, session_token: str | None = None):
        self.base_url = (base_url or os.getenv("DEKO_URL", "http://localhost:8000")).rstrip("/")
        self.password = password or os.getenv("DEKO_ADMIN_PASSWORD")
        self.session_token = session_token or os.getenv("DEKO_SESSION")
        if not self.password and not self.session_token:
            raise ValueError("DekoAdmin requires DEKO_ADMIN_PASSWORD or session_token")
        self._client = httpx.Client(base_url=self.base_url, timeout=30)

    def _headers(self) -> dict[str, str]:
        h: dict[str, str] = {"Content-Type": "application/json"}
        if self.session_token:
            h["Cookie"] = f"deko_session={self.session_token}"
        elif self.password:
            h["X-Admin-Password"] = self.password
        return h

    def _check(self, resp: httpx.Response) -> None:
        if resp.status_code >= 400:
            raise RuntimeError(f"{resp.status_code}: {resp.text}")

    def register_agent(self, name: str) -> dict[str, Any]:
        r = self._client.post("/admin/agents/register", json={"name": name}, headers=self._headers())
        self._check(r)
        return r.json()

    def list_agents(self) -> list[dict[str, Any]]:
        r = self._client.get("/api/admin/agents", headers=self._headers())
        self._check(r)
        return r.json()

    def create_policy(self, name: str, rules: Any, description: str = "") -> dict[str, Any]:
        r = self._client.post("/admin/policies", json={"name": name, "rules": rules, "description": description}, headers=self._headers())
        self._check(r)
        return r.json()

    def list_policies(self) -> list[dict[str, Any]]:
        r = self._client.get("/api/admin/policies", headers=self._headers())
        self._check(r)
        return r.json()

    def delete_policy(self, policy_id: str) -> dict[str, Any]:
        r = self._client.delete(f"/admin/policies/{policy_id}", headers=self._headers())
        self._check(r)
        return r.json()

    def verify_audit(self) -> dict[str, Any]:
        r = self._client.get("/admin/audit/verify", headers=self._headers())
        self._check(r)
        return r.json()
