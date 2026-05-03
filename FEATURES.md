# Deko - Feature List (MVP)

## Phase 1: Project Foundation

### 1. Project Setup
- [ ] F001: Create Cargo.toml with all dependencies
- [ ] F002: Create .gitignore for Rust project
- [ ] F003: Create .env.example with all required env vars
- [ ] F004: Create main.rs with basic Axum server
- [ ] F005: Add tracing/logging setup
- [ ] F006: Create project folder structure (src/, tests/, migrations/)

### 2. Configuration
- [ ] F007: Create config.rs to load env vars
- [ ] F008: Add config validation (fail fast if missing vars)
- [ ] F009: Create config tests

### 3. Database Setup
- [ ] F010: Add sqlx dependency and setup DB pool
- [ ] F011: Create db.rs with pool initialization
- [ ] F012: Create migrations directory
- [ ] F013: Migration 001 - Create agents table
- [ ] F014: Migration 002 - Create actions table
- [ ] F015: Migration 003 - Create verdicts table
- [ ] F016: Migration 004 - Create policies table
- [ ] F017: Migration 005 - Create audit_log table (immutable)
- [ ] F018: Create migration runner script
- [ ] F019: Write DB connection test

## Phase 2: Core Models & Types

### 4. Data Models
- [ ] F020: Create Agent struct (id, name, api_key_hash, created_at)
- [ ] F021: Create Action struct (id, agent_id, intent, payload, screenshot, status, created_at)
- [ ] F022: Create Verdict struct (id, action_id, decision, reason, risk_level, created_at)
- [ ] F023: Create Policy struct (id, name, rules_json, active, created_at)
- [ ] F024: Create AuditLog struct (id, action_id, event, details, created_at)
- [ ] F025: Create RiskLevel enum (Low, Medium, High, Critical)
- [ ] F026: Create VerdictDecision enum (Approved, Denied, Escalate)
- [ ] F027: Create ActionStatus enum (Pending, Processing, Approved, Denied, Escalated, Forwarded)
- [ ] F028: Write model serialization tests

### 5. Error Handling
- [ ] F029: Create AppError enum with all error variants
- [ ] F030: Implement IntoResponse for AppError
- [ ] F031: Create Result alias type
- [ ] F032: Write error handling tests

## Phase 3: Authentication

### 6. API Key Auth
- [ ] F033: Create API key generation utility
- [ ] F034: Create SHA-256 hashing for API keys
- [ ] F035: Create auth middleware (Tower layer)
- [ ] F036: Auth middleware extracts API key from header
- [ ] F037: Auth middleware validates key against DB
- [ ] F038: Auth middleware attaches Agent to request extensions
- [ ] F039: Create auth tests (valid key, invalid key, missing key)
- [ ] F040: Create /auth/register endpoint to create agents
- [ ] F041: Create /auth/revoke endpoint to revoke keys
- [ ] F042: Write auth integration tests

## Phase 4: The Interceptor (Action Ingestion)

### 7. Action Routes
- [ ] F043: Create POST /action endpoint
- [ ] F044: Parse action request body (intent, payload, screenshot_base64, metadata)
- [ ] F045: Validate action input (intent required, screenshot optional)
- [ ] F046: Save action to DB with status=Pending
- [ ] F047: Write audit log entry for action creation
- [ ] F048: Return action_id to caller
- [ ] F049: Create GET /action/:id endpoint
- [ ] F050: Return action details + current verdict
- [ ] F051: Create GET /actions endpoint (list with pagination)
- [ ] F052: Add filtering by agent_id, status, date range
- [ ] F053: Write action route unit tests
- [ ] F054: Write action route integration tests

### 8. Action Polling
- [ ] F055: Create GET /action/:id/status endpoint
- [ ] F056: Return current verdict if ready
- [ ] F057: Return "pending" if still processing
- [ ] F058: Add retry-after header for pending actions
- [ ] F059: Write polling tests

### 9. Action Forwarding (Kill Switch Logic)
- [ ] F060: Create POST /action/:id/forward endpoint
- [ ] F061: Check verdict before forwarding
- [ ] F062: If Approved → forward to target URL
- [ ] F063: If Denied → return 403 with reason
- [ ] F064: If Escalated → return 423 (locked) with message
- [ ] F065: Use reqwest to forward HTTP request
- [ ] F066: Capture forwarded response and log it
- [ ] F067: Write forwarding tests (approve, deny, escalate paths)

## Phase 5: Policy Engine

### 10. Policy Pre-Check
- [ ] F068: Create policy rule parser (JSON → typed rules)
- [ ] F069: Implement keyword matching rule
- [ ] F070: Implement action_type matching rule
- [ ] F071: Implement threshold rule (e.g., max_amount)
- [ ] F072: Implement regex matching rule
- [ ] F073: Implement agent_capability rule (what agent can do)
- [ ] F074: Policy evaluation returns match/no-match
- [ ] F075: If policy match → set risk_level
- [ ] F076: If no policy match → default-deny
- [ ] F077: Create POST /policy endpoint to add policies
- [ ] F078: Create GET /policies endpoint to list policies
- [ ] F079: Create PUT /policy/:id endpoint to update policies
- [ ] F080: Create DELETE /policy/:id endpoint (soft delete, set inactive)
- [ ] F081: Write policy engine unit tests
- [ ] F082: Write policy engine integration tests

## Phase 6: The Eye (OpenAI Vision)

### 11. OpenAI Client
- [ ] F083: Create OpenAI client struct with API key
- [ ] F084: Implement chat completions API call
- [ ] F085: Implement vision (image + text) API call
- [ ] F086: Build vision prompt template (system + user)
- [ ] F087: Parse OpenAI response into structured verdict
- [ ] F088: Add retry logic with exponential backoff (max 2 retries)
- [ ] F089: Add timeout (30s) for OpenAI calls
- [ ] F090: If timeout/unreachable → fail-closed (Denied)
- [ ] F091: Log all OpenAI requests/responses to audit log
- [ ] F092: Write OpenAI client unit tests (mocked)
- [ ] F093: Write OpenAI client integration tests (skipped without API key)

### 12. Verdict Service
- [ ] F094: Create verdict service struct
- [ ] F095: Verdict service runs policy pre-check first
- [ ] F096: If policy pre-check says deny → immediate verdict
- [ ] F097: If policy pre-check passes → call OpenAI Vision
- [ ] F098: Combine policy + LLM verdict into final decision
- [ ] F099: Save verdict to DB
- [ ] F100: Write audit log for verdict
- [ ] F101: Handle LLM response parsing errors → fail-closed
- [ ] F102: Write verdict service tests

## Phase 7: Background Processing

### 13. Action Processor
- [ ] F103: Create tokio background task for processing pending actions
- [ ] F104: Processor polls DB for pending actions (interval-based)
- [ ] F105: Processor calls verdict service for each action
- [ ] F106: Update action status based on verdict
- [ ] F107: Handle processing errors gracefully (fail-closed)
- [ ] F108: Add graceful shutdown for background task
- [ ] F109: Add metrics (actions processed, avg processing time)
- [ ] F110: Write processor tests

### 14. Webhook Notifications
- [ ] F111: Create webhook config in DB/policies
- [ ] F112: Trigger webhook on Denied verdict
- [ ] F113: Trigger webhook on Escalated verdict
- [ ] F114: Include action details + verdict in webhook payload
- [ ] F115: Add retry logic for failed webhooks
- [ ] F116: Log webhook delivery status to audit log
- [ ] F117: Write webhook tests

## Phase 8: Admin Dashboard

### 15. Admin Auth
- [ ] F118: Create admin password hashing (bcrypt)
- [ ] F119: Create admin login endpoint (session/cookie)
- [ ] F120: Create admin logout endpoint
- [ ] F121: Protect admin routes with session middleware
- [ ] F122: Write admin auth tests

### 16. Dashboard Views (Askama Templates)
- [ ] F123: Setup Askama template engine
- [ ] F124: Create base template with layout
- [ ] F125: Create login page template
- [ ] F126: Create dashboard index template (summary stats)
- [ ] F127: Create actions list template (table with filters)
- [ ] F128: Create action detail template (full info + verdict)
- [ ] F129: Create verdict history template
- [ ] F130: Create policy management template
- [ ] F131: Create agent management template
- [ ] F132: Create audit log viewer template
- [ ] F133: Add CSS styling (clean, professional)
- [ ] F134: Add responsive design
- [ ] F135: Write template rendering tests

### 17. Dashboard API Endpoints
- [ ] F136: GET /admin/dashboard - summary stats
- [ ] F137: GET /admin/actions - paginated action list
- [ ] F138: GET /admin/actions/:id - action detail
- [ ] F139: POST /admin/actions/:id/override - admin override (with reason)
- [ ] F140: GET /admin/policies - list policies
- [ ] F141: POST /admin/policies - create policy
- [ ] F142: GET /admin/agents - list agents
- [ ] F143: POST /admin/agents - create agent
- [ ] F144: GET /admin/audit - audit log viewer
- [ ] F145: Write dashboard API tests

## Phase 9: Health & Observability

### 18. Health Checks
- [ ] F146: GET /health endpoint
- [ ] F147: Health check includes DB connectivity
- [ ] F148: Health check includes OpenAI API reachability
- [ ] F149: GET /health/ready endpoint (readiness probe)
- [ ] F150: GET /health/live endpoint (liveness probe)
- [ ] F151: Write health check tests

### 19. Metrics & Logging
- [ ] F152: Structured logging (JSON format for prod)
- [ ] F153: Log request/response cycle with tracing
- [ ] F154: Add request duration metrics
- [ ] F155: Add action count metrics (by status)
- [ ] F156: Add OpenAI call metrics (latency, errors)
- [ ] F157: Export metrics as JSON endpoint
- [ ] F158: Write logging tests

## Phase 10: API Documentation

### 20. OpenAPI Docs
- [ ] F159: Add utoipa for OpenAPI spec generation
- [ ] F160: Document all API endpoints
- [ ] F161: Add request/response schemas
- [ ] F162: Add auth documentation
- [ ] F163: Serve Swagger UI at /docs
- [ ] F164: Write doc generation test

## Phase 11: Deployment

### 21. Docker
- [ ] F165: Create multi-stage Dockerfile
- [ ] F166: Dockerfile produces minimal final image
- [ ] F167: Create docker-compose.yml
- [ ] F168: docker-compose includes app + sqlite volume
- [ ] F169: Add .dockerignore
- [ ] F170: Test docker build and run locally

### 22. Environment & Config
- [ ] F171: Create .env.example with all vars documented
- [ ] F172: Add DEKO_ENV (dev/staging/prod) config
- [ ] F173: Different log levels per environment
- [ ] F174: CORS configuration per environment
- [ ] F175: Write env config tests

## Phase 12: Testing Infrastructure

### 23. Test Framework
- [ ] F176: Setup test database (in-memory SQLite)
- [ ] F177: Create test fixtures/factories
- [ ] F178: Create test helper for app setup
- [ ] F179: Create test helper for auth
- [ ] F180: Create test helper for OpenAI mocking
- [ ] F181: Write CI test runner config (GitHub Actions)
- [ ] F182: Add clippy configuration
- [ ] F183: Add rustfmt configuration

### 24. Integration Tests
- [ ] F184: Full action flow test (submit → process → verdict)
- [ ] F185: Auth flow test (register → auth → access)
- [ ] F186: Policy enforcement test
- [ ] F187: Fail-closed test (DB down → denied)
- [ ] F188: Fail-closed test (OpenAI down → denied)
- [ ] F189: Admin override test
- [ ] F190: Webhook delivery test

## Phase 13: Security Hardening

### 25. Security
- [ ] F191: Rate limiting on API endpoints
- [ ] F192: Request size limits (prevent large payloads)
- [ ] F193: Screenshot size limit (max 10MB)
- [ ] F194: Input sanitization (prevent injection)
- [ ] F195: Secure cookie flags for admin session
- [ ] F196: API key rotation support
- [ ] F197: SQL injection prevention (sqlx parameterized queries)
- [ ] F198: XSS prevention in admin templates

## Phase 14: Polish & MVP Complete

### 26. Final Polish
- [ ] F199: Code review pass (clippy clean)
- [ ] F200: rustfmt pass
- [ ] F201: Remove all TODO comments
- [ ] F202: Add README.md with setup instructions
- [ ] F203: Add CONTRIBUTING.md
- [ ] F204: Add CHANGELOG.md
- [ ] F205: Final integration test run
- [ ] F206: Docker build and smoke test
- [ ] F207: Tag v0.1.0 MVP release

---

## Total Features: 207

## Definition of Done (per feature):
1. Code implemented
2. Unit/integration tests written and passing
3. No clippy warnings
4. Formatted with rustfmt
5. Committed to git with clear message
6. Pushed to remote

## MVP Feature Threshold: F001 through F102 (core functionality)
## Full Feature Set: All 207 features
