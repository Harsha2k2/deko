# Deko - AI Agent Action Watchdog

**Deko** is a security middleware that sits between autonomous AI agents and real-world actions. It intercepts every action an AI agent wants to perform, evaluates it against your security policies and AI-powered risk analysis, and makes a strict decision: **approve**, **deny**, or **escalate for human review**.

Think of it as a firewall for AI agents. No action reaches your systems without passing through Deko first.

---

## Core Principles

| Principle | What It Means |
|---|---|
| **Default-Deny** | Every action is blocked unless explicitly approved |
| **Fail-Closed** | If Deko itself has a problem, actions are denied -- never silently approved |
| **Immutable Audit** | Every decision is permanently logged. Nothing can be erased |
| **Human Override** | A human can override any denial, but the override is itself logged |

---

## How It Works

```
AI Agent ──submit action──▶ Deko ──▶ Policy Engine ──▶ LLM Analysis ──▶ Verdict
                                    (keyword/regex      (Gemini or       Approve /
                                     rules, limits)      OpenAI)         Deny / Escalate
                                                                               │
                                                                    ┌──────────┼──────────┐
                                                                    ▼          ▼          ▼
                                                               Forward    Block     Notify Admin
                                                               to target           (webhook)
```

### The Action Lifecycle

**1. Register an Agent**
An administrator registers each AI agent with Deko. Deko returns a unique API key. This key is how agents identify themselves when submitting actions.

**2. Agent Submits an Action**
The agent sends Deko a description of what it wants to do (the *intent*), optional context (a *payload*), and optionally a screenshot of what it sees. Deko records this immediately and returns a tracking ID.

**3. Deko Evaluates**
In the background, Deko runs the action through two layers:
- **Policy Engine** -- Checks against your rules (blocked keywords, spending limits, regex patterns). A policy match can deny the action instantly.
- **LLM Analysis** -- Sends the intent, payload, and screenshot to an AI model (Gemini or OpenAI) for contextual risk assessment.

**4. Verdict Issued**
Deko records a verdict with a decision (`approved`, `denied`, or `escalated`), a reason, and a risk level (`low`, `medium`, `high`, `critical`).

**5. Agent Polls for Result**
The agent polls `GET /action/{id}/status` until a verdict is ready.

**6. Action Forwarded (if approved)**
If approved, the agent calls `POST /action/{id}/forward` and Deko relays the request to the intended target URL, returning the response.

**7. Admin Override (if denied/escalated)**
A human administrator can review a denied action in the dashboard and override the decision. The override is permanently recorded in the audit log.

---

## Quick Start

### Prerequisites

- **Rust** 1.75 or later
- **SQLite** 3.x
- At least one LLM API key (Google Gemini or OpenAI)

### Run Locally

```bash
git clone git@github.com:Harsha2k2/deko.git
cd deko

# Configure your environment
cp .env.example .env
# Edit .env -- at minimum set your LLM API key and admin password

# Build and run
cargo build --release
cargo run
```

Server starts on `http://localhost:8000`.

### Run with Docker

```bash
docker compose up -d
```

### Verify It's Running

```bash
curl http://localhost:8000/health
# {"status":"healthy","service":"deko","version":"0.1.0","database":"healthy","llm":"healthy"}
```

---

## First-Time Setup

### Step 1: Register Your First Agent

```bash
curl -X POST http://localhost:8000/admin/agents/register \
  -H "X-Admin-Password: changeme" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-agent", "webhook_url": "https://my-agent.example.com/callback"}'
```

Response:
```json
{
  "id": "fd85d0fe-...",
  "name": "my-agent",
  "api_key": "ed3fa62c-...",
  "created_at": "2026-05-03T21:02:54Z"
}
```

Save the `api_key`. It will be used for all future requests from this agent.

### Step 2: Submit an Action

```bash
curl -X POST http://localhost:8000/action \
  -H "X-API-Key: ed3fa62c-..." \
  -H "Content-Type: application/json" \
  -d '{
    "intent": "Transfer $500 to account 12345",
    "payload": {"amount": 500, "recipient": "12345"},
    "target_url": "https://bank.example.com/api/transfer",
    "target_method": "POST"
  }'
```

Response:
```json
{"id": "bd30bccc-...", "status": "pending"}
```

### Step 3: Poll for the Verdict

```bash
curl http://localhost:8000/action/bd30bccc-.../status \
  -H "X-API-Key: ed3fa62c-..."
```

Response (when ready):
```json
{
  "action_id": "bd30bccc-...",
  "status": "approved",
  "verdict": {
    "decision": "approved",
    "reason": "Standard transfer within normal limits. No policy violations detected.",
    "risk_level": "low"
  }
}
```

### Step 4: Forward the Action (if approved)

```bash
curl -X POST http://localhost:8000/action/bd30bccc-.../forward \
  -H "X-API-Key: ed3fa62c-..."
```

---

## API Reference

### Agent Management (Admin Only)

All admin endpoints require the `X-Admin-Password` header.

| Method | Endpoint | Description |
|---|---|---|
| `POST` | `/admin/agents/register` | Register a new AI agent. Returns agent ID and API key. |
| `POST` | `/admin/agents/revoke` | Revoke an agent's API key. |
| `POST` | `/admin/agents/rotate-key` | Generate a new API key for an existing agent. |
| `GET` | `/admin/agents` | List all registered agents (dashboard). |

### Actions (Agent API Key Required)

All action endpoints require the `X-API-Key` header.

| Method | Endpoint | Description |
|---|---|---|
| `POST` | `/action` | Submit a new action for review. |
| `GET` | `/action/{id}` | Get full action details including verdict. |
| `GET` | `/action/{id}/status` | Poll for action verdict. Includes `Retry-After` header if still pending. |
| `POST` | `/action/{id}/forward` | Forward an approved action to its target URL. |
| `GET` | `/actions` | List all actions. Supports `?status=pending` and pagination. |

### Policies (Admin Only)

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/admin/policies` | List all policies. |
| `POST` | `/admin/policies` | Create a new policy. |
| `PUT` | `/admin/policies/{id}` | Update a policy. |
| `DELETE` | `/admin/policies/{id}` | Soft-delete a policy. |

### Admin Dashboard (Admin Only)

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/admin` | Dashboard with summary statistics. |
| `GET` | `/admin/login` | Admin login page. |
| `POST` | `/admin/login` | Authenticate and set session cookie. |
| `POST` | `/admin/logout` | Clear session cookie. |
| `GET` | `/admin/actions` | Browse and filter all actions. |
| `GET` | `/admin/actions/{id}` | View action details, verdict, and audit trail. |
| `POST` | `/admin/actions/{id}/override` | Override a denied or escalated action. |
| `GET` | `/admin/verdicts` | Verdict history with decision filters. |
| `GET` | `/admin/audit` | Full immutable audit log. |

### Health & Observability

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/health` | Full health check (database + LLM connectivity). |
| `GET` | `/health/ready` | Readiness probe for orchestration. |
| `GET` | `/health/live` | Liveness probe for orchestration. |
| `GET` | `/metrics` | JSON metrics (action counts, latency, webhook status). |
| `GET` | `/docs` | Interactive API documentation (Swagger UI). |

---

## Policy Engine

Policies let you define rules that Deko evaluates before calling the LLM. If a policy matches, Deko can deny the action immediately without waiting for LLM analysis.

### Rule Types

| Rule Type | What It Does | Example |
|---|---|---|
| `deny_keyword` | Deny if the intent contains a blocked word | Block any action mentioning "delete all" |
| `regex_deny` | Deny if the intent matches a regex pattern | Block URLs to known bad domains |
| `require_approval` | Flag specific HTTP methods for human review | Require approval for all DELETE requests |
| `max_amount` | Deny if a numeric value exceeds a limit | Block transfers over $10,000 |
| `risk_flag` | Flag keywords for medium-risk escalation | Flag "password reset" for review |

### Creating a Policy

```bash
curl -X POST http://localhost:8000/admin/policies \
  -H "X-Admin-Password: changeme" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Block Destructive Commands",
    "rules": [
      {"type": "deny_keyword", "value": "delete all", "immediate_deny": true},
      {"type": "deny_keyword", "value": "drop table", "immediate_deny": true},
      {"type": "regex_deny", "value": "rm -rf /", "immediate_deny": true}
    ]
  }'
```

---

## LLM Providers

Deko supports multiple AI providers for action analysis. If the primary provider fails, Deko automatically falls back to the secondary provider.

| Provider | Default Model | Strengths |
|---|---|---|
| **Google Gemini** | `gemini-2.0-flash` | Fast, cost-effective, vision support |
| **OpenAI** | `gpt-4o` | High accuracy, vision support |

### Vision Analysis

When an agent submits a screenshot (base64-encoded), Deko sends it to the LLM along with the intent. This lets the model verify what the agent *sees* matches what it *says* it's doing.

### Provider Configuration

```env
LLM_DEFAULT_PROVIDER=gemini
GEMINI_API_KEY=your-key-here
GEMINI_MODEL=gemini-2.0-flash
OPENAI_API_KEY=your-key-here
OPENAI_MODEL=gpt-4o
```

---

## Security

### API Key Authentication
Every agent gets a unique API key. Keys are hashed with SHA-256 before storage -- the raw key is never saved in the database. All action endpoints require a valid `X-API-Key` header.

### Admin Password Protection
The admin dashboard and agent management endpoints are protected by a password configured via `DEKO_ADMIN_PASSWORD`. Authentication is via `X-Admin-Password` header or secure session cookie (`HttpOnly`, `SameSite=Strict`).

### Input Sanitization
All user input is sanitized to prevent XSS attacks. SQL queries use parameterized statements to prevent SQL injection.

### Rate Limiting
Per-IP rate limiting is enabled by default (60 requests per minute, configurable).

### Request Size Limits
- Maximum request body: 512 KB (configurable)
- Maximum screenshot: 10 MB (configurable)

### Fail-Closed Design
If the LLM provider is unreachable, the database connection drops, or any internal error occurs -- the action is **denied**. Deko never allows an action through when it cannot verify it.

---

## Admin Dashboard

Access the dashboard at `http://localhost:8000/admin`. It features:

- **Summary Cards** -- Total actions, pending count, denied count, active agents, active policies
- **Recent Actions** -- Last 20 actions with status badges and risk levels
- **Action Browser** -- Filter by status (pending, approved, denied, escalated)
- **Agent Management** -- Register, view, and revoke agents
- **Policy Management** -- Create and manage policies with a JSON rule editor
- **Verdict History** -- Browse all verdicts, filter by decision type
- **Audit Log** -- Complete, immutable record of every event

---

## Configuration

All configuration is done via environment variables or a `.env` file.

### Server

| Variable | Default | Description |
|---|---|---|
| `DEKO_PORT` | `8000` | HTTP port to listen on |
| `DEKO_ENV` | `dev` | Environment: `dev`, `staging`, or `prod` |

### Database

| Variable | Default | Description |
|---|---|---|
| `DEKO_DATABASE_URL` | `sqlite://data/deko.db` | SQLite database path |

### Security

| Variable | Default | Description |
|---|---|---|
| `DEKO_ADMIN_PASSWORD` | `changeme` | Admin dashboard password (**change this**) |
| `DEKO_API_KEY_SECRET` | *(required)* | Secret for hashing API keys (min 16 chars) |
| `DEKO_ALLOWED_ORIGINS` | `http://localhost:8000` | CORS origins (comma-separated) |
| `DEKO_RATE_LIMIT_PER_MINUTE` | `60` | Max requests per IP per minute |
| `DEKO_MAX_REQUEST_BODY_KB` | `512` | Max request body size in KB |
| `DEKO_MAX_SCREENSHOT_SIZE_MB` | `10` | Max screenshot size in MB |

### LLM

| Variable | Default | Description |
|---|---|---|
| `LLM_DEFAULT_PROVIDER` | `gemini` | Primary LLM: `gemini` or `openai` |
| `LLM_DEFAULT_MODEL` | `gemini-2.0-flash` | Model name |
| `GEMINI_API_KEY` | - | Google Gemini API key |
| `GEMINI_MODEL` | `gemini-2.0-flash` | Gemini model override |
| `GEMINI_TIMEOUT_SECS` | `30` | Gemini API timeout |
| `OPENAI_API_KEY` | - | OpenAI API key |
| `OPENAI_MODEL` | `gpt-4o` | OpenAI model override |
| `OPENAI_TIMEOUT_SECS` | `30` | OpenAI API timeout |

### Notifications

| Variable | Default | Description |
|---|---|---|
| `DEKO_WEBHOOK_URL` | - | URL to notify on denied/escalated actions |

---

## Production Checklist

Before deploying Deko in production:

- [ ] Change `DEKO_ADMIN_PASSWORD` from the default
- [ ] Set `DEKO_API_KEY_SECRET` to a strong, random value (min 16 chars)
- [ ] Configure at least one valid LLM API key
- [ ] Set `DEKO_ENV=prod` for structured JSON logging
- [ ] Configure `DEKO_WEBHOOK_URL` for denied/escalated notifications
- [ ] Set up database backups for the SQLite file
- [ ] Configure CORS `DEKO_ALLOWED_ORIGINS` to restrict access
- [ ] Adjust rate limits for your expected traffic

---

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust |
| Web Framework | Axum |
| Database | SQLite (via SQLx) |
| Templates | Askama (server-side rendered) |
| LLM Providers | Google Gemini, OpenAI GPT-4o |
| API Docs | Utoipa + Swagger UI |
| Logging | Tracing |
| Containerization | Docker (multi-stage build) |
| CI/CD | GitHub Actions |

---

## License

MIT
