"""http proxy — transparent mitm that emulates egress.rs blocklist then deko-checks.

usage:
    deko-guard proxy --port 8080 --targets "https://bank.example.com/*,https://api.*"
    # then in agent code:
    export HTTP_PROXY=http://localhost:8080
    export HTTPS_PROXY=http://localhost:8080
"""
from __future__ import annotations
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse
import httpx

# reuse python-side blocklist for fail-fast (mirrors rust egress.rs)
_BLOCKED_SUBSTRINGS = ["169.254.169.254", "127.0.0.1", "localhost", "10.", "192.168.", "172.16."]

def _is_blocked_local(url: str) -> bool:
    try:
        host = urlparse(url).hostname or ""
        if host == "localhost":
            return True
        for bad in _BLOCKED_SUBSTRINGS:
            if bad in url:
                # naive — real check is ip-based, but fail-fast is better than nothing
                if bad in ("10.", "192.168.") and host.startswith(bad):
                    return True
                if bad not in ("10.", "192.168."):
                    return True
        return False
    except Exception:
        return False

class ProxyHandler(BaseHTTPRequestHandler):
    deko = None  # set by server factory
    targets: list[str] = []

    def _should_intercept(self, url: str) -> bool:
        if not self.targets:
            return True  # intercept all by default in 2.0.0a0
        for pat in self.targets:
            # very small glob: "*" suffix
            if pat.endswith("*") and url.startswith(pat[:-1]):
                return True
            if pat == url or pat in url:
                return True
        return False

    def _handle(self, method: str):
        # absolute url when used as proxy, or path when direct
        raw_url = self.path
        if not raw_url.startswith("http"):
            host = self.headers.get("Host", "")
            scheme = "https" if self.headers.get("X-Forwarded-Proto") == "https" else "http"
            raw_url = f"{scheme}://{host}{raw_url}"

        content_length = int(self.headers.get("Content-Length", 0) or 0)
        body = self.rfile.read(content_length) if content_length else b""

        if _is_blocked_local(raw_url):
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": f"proxy blocked private target: {raw_url}"}).encode())
            return

        if not self._should_intercept(raw_url):
            # passthrough without deko
            self._forward(method, raw_url, body)
            return

        # deko check
        try:
            intent = f"{method} {raw_url}"
            payload = body.decode(errors="ignore")[:2000] if body else None
            verdict = self.deko.check(intent=intent, payload=payload, target_url=raw_url, target_method=method, wait=True, timeout=10)  # type: ignore
        except Exception as e:
            self.send_response(403)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": f"deko check failed: {e}"}).encode())
            return

        if verdict.decision == "denied":
            self.send_response(403)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": f"deko denied: {verdict.reason}", "risk_level": verdict.risk_level}).encode())
            return
        if verdict.decision == "escalate":
            self.send_response(423)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": f"deko escalated: {verdict.reason}"}).encode())
            return

        # approved — forward and return honest result via deko forward, or direct
        # prefer deko forward for audit completeness if we have an action_id
        try:
            fwd = self.deko.forward(verdict.action_id)  # type: ignore
            if fwd.forwarded:
                self.send_response(fwd.target_status or 200)
                self.send_header("Content-Type", "text/plain")
                self.end_headers()
                self.wfile.write((fwd.target_response or "").encode())
                return
        except Exception:
            pass
        # fallback direct forward
        self._forward(method, raw_url, body)

    def _forward(self, method: str, url: str, body: bytes):
        try:
            with httpx.Client(follow_redirects=False, timeout=10) as c:
                # copy headers except proxy ones
                headers = {k: v for k, v in self.headers.items() if k.lower() not in ("host", "proxy-connection", "connection")}
                resp = c.request(method, url, content=body, headers=headers)
                self.send_response(resp.status_code)
                for k, v in resp.headers.items():
                    if k.lower() not in ("content-encoding", "transfer-encoding", "connection"):
                        self.send_header(k, v)
                self.end_headers()
                self.wfile.write(resp.content)
        except Exception as e:
            self.send_response(502)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": f"proxy forward failed: {e}"}).encode())

    def do_GET(self): self._handle("GET")
    def do_POST(self): self._handle("POST")
    def do_PUT(self): self._handle("PUT")
    def do_DELETE(self): self._handle("DELETE")
    def do_PATCH(self): self._handle("PATCH")
    def log_message(self, format, *args):  # quiet
        pass

def run_proxy(port: int = 8080, targets: list[str] | None = None, deko=None) -> None:
    if deko is None:
        try:
            from deko_guard.client.facade import Deko
            deko = Deko()
        except Exception as e:
            print(f"cannot create Deko client: {e}")
            return
    ProxyHandler.deko = deko
    ProxyHandler.targets = targets or []
    server = ThreadingHTTPServer(("0.0.0.0", port), ProxyHandler)
    print(f"deko-guard proxy listening on :{port} (targets={targets or 'all'})")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.shutdown()
