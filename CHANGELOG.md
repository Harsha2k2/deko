# Changelog

All notable changes to Deko will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-05-03

### Added

#### Core Engine
- AI Agent Action Watchdog middleware for autonomous agent safety
- Multi-provider LLM support (Gemini, OpenAI) with automatic fallback
- Policy engine with keyword matching, regex, threshold, and capability rules
- Default-deny and fail-closed security model
- Action ingestion, processing, and verdict pipeline
- Background action processor with configurable polling interval

#### API Endpoints
- POST /action - Submit actions for review
- GET /action/:id - Get action details
- GET /action/:id/status - Poll for verdict (with Retry-After header)
- POST /action/:id/forward - Forward approved actions to target
- GET /actions - List actions with pagination and filtering
- POST /auth/register - Register new agents
- POST /auth/revoke - Revoke agent API keys
- POST /auth/rotate-key - Rotate agent API keys
- GET /admin/agents - Agent management dashboard
- GET /admin/policies - Policy management dashboard
- POST /admin/policies - Create policies
- PUT /admin/policies/:id - Update policies
- DELETE /admin/policies/:id - Soft delete policies
- GET /admin/verdicts - Verdict history viewer
- GET /admin/audit - Audit log viewer
- GET /admin/actions - Admin actions list
- GET /admin/actions/:id - Action detail view
- POST /admin/actions/:id/override - Admin override denied/escalated actions
- GET /health - Health check (DB + LLM connectivity)
- GET /health/ready - Readiness probe
- GET /health/live - Liveness probe
- GET /metrics - Prometheus-style JSON metrics

#### Admin Dashboard
- Server-side rendered admin UI with Askama templates
- Login page with password authentication
- Dashboard with summary statistics
- Action list with status filters
- Action detail view with verdict and audit trail
- Agent management with register/revoke/rotate-key
- Policy management with JSON rule editor
- Verdict history with decision filters
- Audit log viewer with event type filters
- Responsive CSS styling

#### Security
- API key authentication middleware (SHA-256 hashed)
- Admin password protection for dashboard routes
- Rate limiting per IP address
- Request body size limits
- Screenshot size limits (10MB default)
- Input sanitization (XSS prevention)
- Parameterized SQL queries (SQL injection prevention)
- Secure cookie support

#### Observability
- Structured JSON logging for production
- Request/response tracing with duration metrics
- Action count metrics by status
- LLM call metrics (latency, errors)
- Webhook delivery tracking
- JSON metrics endpoint at /metrics

#### Infrastructure
- Multi-stage Dockerfile for minimal images
- Docker Compose with SQLite volume
- OpenAPI/Swagger documentation at /docs
- GitHub Actions CI configuration
- Environment-based configuration (dev/staging/prod)
- Graceful shutdown handling

### Security
- Default-deny: all actions denied without explicit approval
- Fail-closed: LLM failures result in denial
- Policy pre-check before LLM evaluation
- Immutable audit log for all actions and verdicts
