"""cli — `deko-guard` entry point."""
import argparse
import os
import sys

def main():
    p = argparse.ArgumentParser(prog="deko-guard", description="control plane for ai agent actions")
    sub = p.add_subparsers(dest="cmd")

    init_p = sub.add_parser("init", help="scaffold DEKO_URL/DEKO_API_KEY in .env")
    init_p.add_argument("--lang", choices=["python", "ts"], default="python")

    mcp_p = sub.add_parser("mcp", help="mcp gate proxy")
    mcp_p.add_argument("--upstream", required=True, help="upstream mcp command, e.g. \"npx @modelcontextprotocol/server-filesystem /tmp\"")
    mcp_p.add_argument("--transport", choices=["stdio"], default="stdio")

    proxy_p = sub.add_parser("proxy", help="http proxy emulating egress guard")
    proxy_p.add_argument("--port", type=int, default=8080)
    proxy_p.add_argument("--targets", default="", help="comma-separated url globs, e.g. \"https://bank.example.com/*\"")

    args = p.parse_args()

    if args.cmd == "init":
        env_path = os.path.join(os.getcwd(), ".env")
        if os.path.exists(env_path):
            print(f"{env_path} already exists — not overwriting")
        else:
            with open(env_path, "w") as f:
                f.write("DEKO_URL=http://localhost:8000\nDEKO_API_KEY=\n")
            print("wrote .env — fill DEKO_API_KEY from `POST /admin/agents/register`")
        # also scaffold example file
        ex_dir = "examples"
        os.makedirs(ex_dir, exist_ok=True)
        if args.lang == "python":
            with open(os.path.join(ex_dir, "guarded_tool.py"), "w") as f:
                f.write('from deko_guard import Deko\ndeko = Deko()\n@deko.guard\ndef my_tool(x: int): return x*2\n')
            print("wrote examples/guarded_tool.py")
        else:
            with open(os.path.join(ex_dir, "guarded.ts"), "w") as f:
                f.write('import { Deko } from "deko-guard"; const deko = new Deko();\n')
            print("wrote examples/guarded.ts")

    elif args.cmd == "mcp":
        from deko_guard.adapters.mcp import run_gate
        sys.exit(run_gate(args.upstream, transport=args.transport))

    elif args.cmd == "proxy":
        from deko_guard.proxy.http_proxy import run_proxy
        targets = [t.strip() for t in args.targets.split(",") if t.strip()] if args.targets else None
        run_proxy(port=args.port, targets=targets)

    else:
        p.print_help()

if __name__ == "__main__":
    main()
