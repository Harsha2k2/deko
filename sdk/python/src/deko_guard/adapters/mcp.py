"""mcp gate — stdio/sse proxy that intercepts tools/call.

protocol: json-rpc 2.0 lines over stdio (claude desktop) or sse.
gate sits between mcp client (claude) and upstream mcp server (filesystem etc).
every tools/call is turned into a deko action; denied/escalate never reach upstream.

usage:
    deko-guard mcp --upstream "npx @modelcontextprotocol/server-filesystem /tmp"
    deko-guard mcp --upstream "python -m my_mcp_server" --transport stdio
"""
from __future__ import annotations
import json
import subprocess
import sys
import threading
from typing import Any

def _should_gate(method: str | None) -> bool:
    return method == "tools/call"

def _derive_intent(params: dict[str, Any] | None) -> tuple[str, str]:
    if not params:
        return "mcp tools/call", "{}"
    name = params.get("name", "unknown_tool")
    args = params.get("arguments", params.get("input", {}))
    try:
        payload = json.dumps({"tool": name, "arguments": args}, default=str)
    except Exception:
        payload = str(args)
    intent = f"mcp tool {name}"
    # include first arg values for policy matching
    try:
        arg_str = ", ".join(f"{k}={v!r}" for k, v in (args.items() if isinstance(args, dict) else {}))
        if arg_str:
            intent += f"({arg_str[:200]})"
    except Exception:
        pass
    return intent, payload

class McpGate:
    def __init__(self, upstream_cmd: str, deko=None):
        self.upstream_cmd = upstream_cmd
        # lazy import to avoid hard dep on deko when gate not used
        if deko is None:
            try:
                from deko_guard.client.facade import Deko
                deko = Deko()
            except Exception as e:
                print(f"deko mcp gate: could not create Deko client: {e}", file=sys.stderr)
                deko = None
        self.deko = deko
        self.proc: subprocess.Popen | None = None

    def run_stdio(self) -> int:
        import shlex
        try:
            self.proc = subprocess.Popen(
                shlex.split(self.upstream_cmd),
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=sys.stderr,
                text=True,
                bufsize=1,
            )
        except Exception as e:
            print(f"failed to spawn upstream '{self.upstream_cmd}': {e}", file=sys.stderr)
            return 1

        assert self.proc.stdin and self.proc.stdout

        # thread to forward upstream -> client (stdout)
        def upstream_to_client():
            assert self.proc and self.proc.stdout
            for line in self.proc.stdout:
                sys.stdout.write(line)
                sys.stdout.flush()

        t = threading.Thread(target=upstream_to_client, daemon=True)
        t.start()

        # main loop: client -> upstream (with intercept)
        try:
            for line in sys.stdin:
                if not line.strip():
                    continue
                try:
                    msg = json.loads(line)
                except Exception:
                    # not json — passthrough
                    self.proc.stdin.write(line)
                    self.proc.stdin.flush()
                    continue

                method = msg.get("method")
                msg_id = msg.get("id")

                if _should_gate(method) and self.deko is not None:
                    params = msg.get("params", {})
                    intent, payload = _derive_intent(params)
                    try:
                        verdict = self.deko.check(intent=intent, payload=payload, wait=True, timeout=15)
                    except Exception as e:
                        # fail-closed: on deko error, deny
                        err = {"jsonrpc": "2.0", "id": msg_id, "error": {"code": -32600, "message": f"deko check failed: {e}"}}
                        sys.stdout.write(json.dumps(err) + "\n")
                        sys.stdout.flush()
                        continue

                    if verdict.decision == "denied":
                        err = {
                            "jsonrpc": "2.0",
                            "id": msg_id,
                            "error": {
                                "code": -32600,
                                "message": f"deko denied: {verdict.reason}",
                                "data": {"reason": verdict.reason, "risk_level": verdict.risk_level, "policy_matched": verdict.policy_matched},
                            },
                        }
                        sys.stdout.write(json.dumps(err) + "\n")
                        sys.stdout.flush()
                        continue
                    if verdict.decision == "escalate":
                        err = {
                            "jsonrpc": "2.0",
                            "id": msg_id,
                            "error": {
                                "code": -32001,
                                "message": f"deko escalated: {verdict.reason} — requires human review",
                                "data": {"reason": verdict.reason, "risk_level": verdict.risk_level},
                            },
                        }
                        sys.stdout.write(json.dumps(err) + "\n")
                        sys.stdout.flush()
                        continue
                    # approved → fall through to forward

                # forward to upstream
                self.proc.stdin.write(line)
                self.proc.stdin.flush()
        except KeyboardInterrupt:
            pass
        finally:
            try:
                if self.proc:
                    self.proc.terminate()
            except Exception:
                pass
        return 0

def run_gate(upstream_cmd: str, transport: str = "stdio") -> int:
    if transport != "stdio":
        print(f"transport {transport} not yet implemented — only stdio is supported in 2.0.0a0", file=sys.stderr)
        return 1
    gate = McpGate(upstream_cmd)
    return gate.run_stdio()
