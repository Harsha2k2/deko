"""cli — `deko-guard init` + `deko-guard mcp` placeholder."""
import argparse
import os

def main():
    p = argparse.ArgumentParser(prog="deko-guard")
    sub = p.add_subparsers(dest="cmd")
    sub.add_parser("init", help="scaffold DEKO_URL/DEKO_API_KEY in .env")
    sub.add_parser("mcp", help="mcp gate proxy (phase 3)")
    args = p.parse_args()
    if args.cmd == "init":
        env_path = os.path.join(os.getcwd(), ".env")
        if os.path.exists(env_path):
            print(f"{env_path} already exists — not overwriting")
        else:
            with open(env_path, "w") as f:
                f.write("DEKO_URL=http://localhost:8000\nDEKO_API_KEY=\n")
            print("wrote .env — fill DEKO_API_KEY from `POST /admin/agents/register`")
    elif args.cmd == "mcp":
        print("mcp gate ships in phase 3 — see docs/sdk-architecture.md §8")
    else:
        p.print_help()

if __name__ == "__main__":
    main()
