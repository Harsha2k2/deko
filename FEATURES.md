# Deko - Feature List (MVP)

## Phase 1: Project Foundation

### 1. Project Setup
- [x] F001: Create Cargo.toml with all dependencies
- [x] F002: Create .gitignore for Rust project
- [x] F003: Create .env.example with all required env vars
- [x] F004: Create main.rs with basic Axum server
- [x] F005: Add tracing/logging setup
- [x] F006: Create project folder structure (src/, tests/, migrations/)

### 2. Configuration
- [x] F007: Create config.rs to load env vars
- [x] F008: Add config validation (fail fast if missing vars)
- [x] F009: Create config tests

### 3. Database Setup
- [x] F010: Add sqlx dependency and setup DB pool
- [x] F011: Create db.rs with pool initialization
- [x] F012: Create migrations directory
- [x] F013: Migration 001 - Create agents table
- [x] F014: Migration 002 - Create actions table
- [x] F015: Migration 003 - Create verdicts table
- [x] F016: Migration 004 - Create policies table
- [x] F017: Migration 005 - Create audit_log table (immutable)
- [x] F018: Create migration runner script
- [x] F019: Write DB connection test

## Phase 2: Core Models & Types

### 4. Data Models
- [x] F020: Create Agent struct (id, name, api_key_hash, created_at)
- [x] F021: Create Action struct (id, agent_id, intent, payload, screenshot, status, created_at)
- [x] F022: Create Verdict struct (id, action_id, decision, reason, risk_level, created_at)
- [x] F023: Create Policy struct (id, name, rules_json, active, created_at)
- [x] F024: Create AuditLog struct (id, action_id, event, details, created_at)
- [x] F025: Create RiskLevel enum (Low, Medium, High, Critical)
- [x] F026: Create VerdictDecision enum (Approved, Denied, Escalate)
- [x] F027: Create ActionStatus enum (Pending, Processing, Approved, Denied, Escalated, Forwarded)
- [x] F028: Write model serialization tests

### 5. Error Handling
- [x] F029: Create AppError enum with all error variants
- [x] F030: Implement IntoResponse for AppError
- [x] F031: Create Result alias type
- [x] F032: Write error handling tests

## Phase 3: Authentication

### 6. API Key Auth
- [x] F033: Create API key generation utility
- [x] F034: Create SHA-256 hashing for API keys
- [x] F035: Create auth middleware (Tower layer)
- [x] F036: Auth middleware extracts API key from header
- [x] F037: Auth middleware validates key against DB
- [x] F038: Auth middleware attaches Agent to request extensions
- [x] F039: Create auth tests (valid key, invalid key, missing key)
- [x] F040: Create /auth/register endpoint to create agents
- [x] F041: Create /auth/revoke endpoint to revoke keys
- [x] F042: Write auth integration tests

## Phase 4: The Interceptor (Action Ingestion)

### 7. Action Routes
- [x] F043: Create POST /action endpoint
- [x] F044: Parse action request body (intent, payload, screenshot_base64, metadata)
- [x] F045: Validate action input (intent required, screenshot optional)
- [x] F046: Save action to DB with status=Pending
- [x] F047: Write audit log entry for action creation
- [x] F048: Return action_id to caller
- [x] F049: Create GET /action/:id endpoint
- [x] F050: Return action details + current verdict
- [x] F051: Create GET /actions endpoint (list with pagination)
- [x] F052: Add filtering by agent_id, status, date range
- [x] F053: Write action route unit tests
- [x] F054: Write action route integration tests

### 8. Action Polling
- [x] F055: Create GET /action/:id/status endpoint
- [x] F056: Return current verdict if ready
- [x] F057: Return "pending" if still processing
- [x] F058: Add retry-after header for pending actions
- [x] F059: Write polling tests

### 9. Action Forwarding (Kill Switch Logic)
- [x] F060: Create POST /action/:id/forward endpoint
- [x] F061: Check verdict before forwarding
- [x] F062: If Approved → forward to target URL
- [x] F063: If Denied → return 403 with reason
- [x] F064: If Escalated → return 423 (locked) with message
- [x] F065: Use reqwest to forward HTTP request
- [x] F066: Capture forwarded response and log it
- [x] F067: Write forwarding tests (approve, deny, escalate paths)

## Phase 5: Policy Engine

### 10. Policy Pre-Check
- [x] F068: Create policy rule parser (JSON → typed rules)
- [x] F069: Implement keyword matching rule
- [x] F070: Implement action_type matching rule
- [x] F071: Implement threshold rule (e.g., max_amount)
- [x] F072: Implement regex matching rule
- [x] F073: Implement agent_capability rule (what agent can do)
- [x] F074: Policy evaluation returns match/no-match
- [x] F075: If policy match → set risk_level
- [x] F076: If no policy match → default-deny
- [x] F077: Create POST /policy endpoint to add policies
- [x] F078: Create GET /policies endpoint to list policies
- [x] F079: Create PUT /policy/:id endpoint to update policies
- [x] F080: Create DELETE /policy/:id endpoint (soft delete, set inactive)
- [x] F081: Write policy engine unit tests
- [x] F082: Write policy engine integration tests

## Phase 6: The Eye (OpenAI Vision)

### 11. OpenAI Client
- [x] F083: Create OpenAI client struct with API key
- [x] F084: Implement chat completions API call
- [x] F085: Implement vision (image + text) API call
- [x] F086: Build vision prompt template (system + user)
- [x] F087: Parse OpenAI response into structured verdict
- [x] F088: Add retry logic with exponential backoff (max 2 retries)
- [x] F089: Add timeout (30s) for OpenAI calls
- [x] F090: If timeout/unreachable → fail-closed (Denied)
- [x] F091: Log all OpenAI requests/responses to audit log
- [x] F092: Write OpenAI client unit tests (mocked)
- [x] F093: Write OpenAI client integration tests (skipped without API key)

### 12. Verdict Service
- [x] F094: Create verdict service struct
- [x] F095: Verdict service runs policy pre-check first
- [x] F096: If policy pre-check says deny → immediate verdict
- [x] F097: If policy pre-check passes → call OpenAI Vision
- [x] F098: Combine policy + LLM verdict into final decision
- [x] F099: Save verdict to DB
- [x] F100: Write audit log for verdict
- [x] F101: Handle LLM response parsing errors → fail-closed
- [x] F102: Write verdict service tests

## Phase 7: Background Processing

### 13. Action Processor
- [x] F103: Create tokio background task for processing pending actions
- [x] F104: Processor polls DB for pending actions (interval-based)
- [x] F105: Processor calls verdict service for each action
- [x] F106: Update action status based on verdict
- [x] F107: Handle processing errors gracefully (fail-closed)
- [x] F108: Add graceful shutdown for background task
- [x] F109: Add metrics (actions processed, avg processing time)
- [x] F110: Write processor tests

### 14. Webhook Notifications
- [x] F111: Create webhook config in DB/policies
- [x] F112: Trigger webhook on Denied verdict
- [x] F113: Trigger webhook on Escalated verdict
- [x] F114: Include action details + verdict in webhook payload
- [x] F115: Add retry logic for failed webhooks
- [x] F116: Log webhook delivery status to audit log
- [x] F117: Write webhook tests

## Phase 8: Admin Dashboard

### 15. Admin Auth
- [x] F118: Create admin password hashing (bcrypt)
- [x] F119: Create admin login endpoint (session/cookie)
- [x] F120: Create admin logout endpoint
- [x] F121: Protect admin routes with session middleware
- [x] F122: Write admin auth tests

### 16. Dashboard Views (Askama Templates)
- [x] F123: Setup Askama template engine
- [x] F124: Create base template with layout
- [x] F125: Create login page template
- [x] F126: Create dashboard index template (summary stats)
- [x] F127: Create actions list template (table with filters)
- [x] F128: Create action detail template (full info + verdict)
- [x] F129: Create verdict history template
- [x] F130: Create policy management template
- [x] F131: Create agent management template
- [x] F132: Create audit log viewer template
- [x] F133: Add CSS styling (clean, professional)
- [x] F134: Add responsive design
- [x] F135: Write template rendering tests

### 17. Dashboard API Endpoints
- [x] F136: GET /admin/dashboard - summary stats
- [x] F137: GET /admin/actions - paginated action list
- [x] F138: GET /admin/actions/:id - action detail
- [x] F139: POST /admin/actions/:id/override - admin override (with reason)
- [x] F140: GET /admin/policies - list policies
- [x] F141: POST /admin/policies - create policy
- [x] F142: GET /admin/agents - list agents
- [x] F143: POST /admin/agents - create agent
- [x] F144: GET /admin/audit - audit log viewer
- [x] F145: Write dashboard API tests

## Phase 9: Health & Observability

### 18. Health Checks
- [x] F146: GET /health endpoint
- [x] F147: Health check includes DB connectivity
- [x] F148: Health check includes OpenAI API reachability
- [x] F149: GET /health/ready endpoint (readiness probe)
- [x] F150: GET /health/live endpoint (liveness probe)
- [x] F151: Write health check tests

### 19. Metrics & Logging
- [x] F152: Structured logging (JSON format for prod)
- [x] F153: Log request/response cycle with tracing
- [x] F154: Add request duration metrics
- [x] F155: Add action count metrics (by status)
- [x] F156: Add OpenAI call metrics (latency, errors)
- [x] F157: Export metrics as JSON endpoint
- [x] F158: Write logging tests

## Phase 10: API Documentation

### 20. OpenAPI Docs
- [x] F159: Add utoipa for OpenAPI spec generation
- [x] F160: Document all API endpoints
- [x] F161: Add request/response schemas
- [x] F162: Add auth documentation
- [x] F163: Serve Swagger UI at /docs
- [x] F164: Write doc generation test

## Phase 11: Deployment

### 21. Docker
- [x] F165: Create multi-stage Dockerfile
- [x] F166: Dockerfile produces minimal final image
- [x] F167: Create docker-compose.yml
- [x] F168: docker-compose includes app + sqlite volume
- [x] F169: Add .dockerignore
- [x] F170: Test docker build and run locally

### 22. Environment & Config
- [x] F171: Create .env.example with all vars documented
- [x] F172: Add DEKO_ENV (dev/staging/prod) config
- [x] F173: Different log levels per environment
- [x] F174: CORS configuration per environment
- [x] F175: Write env config tests

## Phase 12: Testing Infrastructure

### 23. Test Framework
- [x] F176: Setup test database (in-memory SQLite)
- [x] F177: Create test fixtures/factories
- [x] F178: Create test helper for app setup
- [x] F179: Create test helper for auth
- [x] F180: Create test helper for OpenAI mocking
- [x] F181: Write CI test runner config (GitHub Actions)
- [x] F182: Add clippy configuration
- [x] F183: Add rustfmt configuration

### 24. Integration Tests
- [x] F184: Full action flow test (submit → process → verdict)
- [x] F185: Auth flow test (register → auth → access)
- [x] F186: Policy enforcement test
- [x] F187: Fail-closed test (DB down → denied)
- [x] F188: Fail-closed test (OpenAI down → denied)
- [x] F189: Admin override test
- [x] F190: Webhook delivery test

## Phase 13: Security Hardening

### 25. Security
- [x] F191: Rate limiting on API endpoints
- [x] F192: Request size limits (prevent large payloads)
- [x] F193: Screenshot size limit (max 10MB)
- [x] F194: Input sanitization (prevent injection)
- [x] F195: Secure cookie flags for admin session
- [x] F196: API key rotation support
- [x] F197: SQL injection prevention (sqlx parameterized queries)
- [x] F198: XSS prevention in admin templates

## Phase 14: Polish & MVP Complete

### 26. Final Polish
- [x] F199: Code review pass (clippy clean)
- [x] F200: rustfmt pass
- [x] F201: Remove all TODO comments
- [x] F202: Add README.md with setup instructions
- [x] F203: Add CONTRIBUTING.md
- [x] F204: Add CHANGELOG.md
- [x] F205: Final integration test run
- [x] F206: Docker build and smoke test
- [x] F207: Tag v0.1.0 MVP release

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
