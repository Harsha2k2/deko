# Deko - AI Agent Action Watchdog

A strict middleware watchdog that sits between autonomous AI agents and real-world actions. Deko intercepts every action, evaluates it against policies and AI vision analysis, and decides whether to **approve**, **deny**, or **escalate**.

## Architecture

```
AI Agent → Deko Middleware → Policy Engine → LLM Vision Check → Verdict → Action Forwarded or Killed
```

## Multi-Provider LLM Support

Deko supports multiple LLM providers with automatic fallback:

- **Google Gemini** (default) - Fast, cost-effective
- **OpenAI GPT-4o** - High accuracy
- Switch between providers via config or per-action routing

## Quick Start

### Prerequisites

- Rust 1.75+
- At least one LLM API key (Gemini or OpenAI)

### Setup

```bash
# Clone
git clone git@github.com:Harsha2k2/deko.git
cd deko

# Configure
cp .env.example .env
# Edit .env with your API keys

# Build
cargo build --release

# Run
cargo run
```

### Docker

```bash
docker compose up -d
```

## API

### Register an Agent

```bash
curl -X POST http://localhost:8000/admin/agents/register \
  -H "X-Admin-Password: changeme" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-agent"}'
```

### Submit an Action

```bash
curl -X POST http://localhost:8000/action \
  -H "X-API-Key: <agent-api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "intent": "Buy 10 shares of AAPL",
    "payload": "{\"symbol\": \"AAPL\", \"quantity\": 10}",
    "target_url": "https://broker.example.com/api/buy",
    "target_method": "POST"
  }'
```

### Check Verdict

```bash
curl http://localhost:8000/action/<action-id>/status \
  -H "X-API-Key: <agent-api-key>"
```

### Forward (if approved)

```bash
curl -X POST http://localhost:8000/action/<action-id>/forward \
  -H "X-API-Key: <agent-api-key>"
```

### Admin Dashboard

```bash
# Login
curl -X POST http://localhost:8000/admin/login \
  -H "Content-Type: application/json" \
  -d '{"password": "changeme"}'

# Stats
curl http://localhost:8000/admin

# View actions
curl "http://localhost:8000/admin/actions?status=denied"

# Override a denied action
curl -X POST http://localhost:8000/admin/actions/<id>/override \
  -H "Content-Type: application/json" \
  -d '{"reason": "Reviewed and safe"}'
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DEKO_PORT` | 8000 | Server port |
| `DEKO_ENV` | dev | Environment (dev/staging/prod) |
| `DEKO_ADMIN_PASSWORD` | changeme | Admin dashboard password |
| `DEKO_DATABASE_URL` | sqlite://data/deko.db | Database URL |
| `LLM_DEFAULT_PROVIDER` | gemini | Default LLM (gemini/openai) |
| `LLM_DEFAULT_MODEL` | gemini-2.0-flash | Default model name |
| `GEMINI_API_KEY` | - | Google Gemini API key |
| `GEMINI_MODEL` | gemini-2.0-flash | Gemini model to use |
| `OPENAI_API_KEY` | - | OpenAI API key |
| `OPENAI_MODEL` | gpt-4o | OpenAI model to use |
| `DEKO_API_KEY_SECRET` | - | Secret for hashing agent API keys |
| `DEKO_WEBHOOK_URL` | - | URL for denied/escalated notifications |

## Features

- **Default-Deny**: Everything is blocked unless explicitly approved
- **Fail-Closed**: System failures result in denial, never approval
- **Multi-Layer Validation**: Policy rules → LLM analysis → Forwarding
- **Immutable Audit Log**: All actions and verdicts are permanently recorded
- **Admin Override**: Human can override denials with documented reason
- **Multi-Provider LLM**: Automatic fallback if primary provider fails
- **Policy Templates**: JSON-based rules for keyword, regex, amount limits

## License

MIT
