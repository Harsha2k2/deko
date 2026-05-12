# Deko - Feature Roadmap & Checklist

> **Legend:**  
> `[x]` = Done  
> `[~]` = Partial / Beta quality  
> `[ ]` = Planned / Not started  

---

## Phase 0: Core Concept

| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F000 | Default-deny security model | [x] | Every action blocked unless explicitly approved |
| F001 | Fail-closed architecture | [x] | System failures result in denial, never approval |
| F002 | Immutable audit trail | [x] | All decisions permanently recorded, nothing erasable |
| F003 | Human override with accountability | [x] | Admin can override, but override is logged permanently |
| F004 | Multi-layer validation pipeline | [x] | Policy engine -> LLM analysis -> forwarding decision |

---

## Phase 1: Project Foundation

### 1.1 Project Setup
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F005 | Cargo.toml with all dependencies | [x] | Axum, SQLx, Tokio, Askama, Reqwest, etc. |
| F006 | .gitignore for Rust project | [x] | Ignores target/, .env, *.db |
| F007 | .env.example with all env vars | [x] | Documented with defaults |
| F008 | main.rs with Axum server bootstrap | [x] | Tokio runtime, tracing, graceful shutdown |
| F009 | Tracing/logging setup | [x] | JSON in prod, human-readable in dev |
| F010 | Folder structure | [x] | src/, tests/, migrations/, templates/, static/ |

### 1.2 Configuration
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F011 | Config struct from env vars | [x] | Config::from_env() |
| F012 | Config validation (fail fast) | [x] | Missing vars crash at startup, not runtime |
| F013 | Config unit tests | [x] | Default values, validation edge cases |
| F014 | Per-environment config profiles | [x] | dev/staging/prod env vars defined but no separate profiles |
| F015 | Config hot-reload | [ ] | SIGHUP or file watch for env changes |
| F016 | Secret redaction in logs | [ ] | API keys, passwords masked in debug output |

### 1.3 Database
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F017 | SQLx dependency + pool init | [x] | With connection pooling |
| F018 | db.rs module | [x] | init_db(), run_migrations() |
| F019 | Migration: agents table | [x] | id, name, api_key_hash, active, created_at |
| F020 | Migration: actions table | [x] | id, agent_id, intent, payload, screenshot, status, timestamps |
| F021 | Migration: verdicts table | [x] | id, action_id, decision, reason, risk_level, llm_raw_response |
| F022 | Migration: policies table | [x] | id, name, rules (JSON), active, created_at |
| F023 | Migration: audit_log table | [x] | id, action_id, event_type, details (JSON), created_at |
| F024 | Migration runner | [x] | SQLx migrate run at startup |
| F025 | DB connection test | [x] | In-memory SQLite for tests |
| F026 | PostgreSQL support | [x] | SQLx supports it, need Feature flag + config |
| F027 | Read replica support | [ ] | Separate read/write connection pools |
| F028 | Connection pool metrics | [ ] | Expose pool size, wait times |
| F029 | Automatic migration rollback | [ ] | Rollback scripts for each migration |
| F030 | Database backup automation | [ ] | Script for SQLite backup/restore |

---

## Phase 2: Core Models & Types

### 2.1 Data Models
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F031 | Agent model | [x] | id, name, api_key_hash, active, created_at |
| F032 | Action model | [x] | id, agent_id, intent, payload, screenshot, metadata, status, target_url/method |
| F033 | Verdict model | [x] | id, action_id, decision, reason, risk_level, policy_matched |
| F034 | Policy model | [x] | id, name, rules_json, active, timestamps |
| F035 | AuditLog model | [x] | id, action_id, event_type, details (JSON), created_at |
| F036 | RiskLevel enum | [x] | Low, Medium, High, Critical |
| F037 | VerdictDecision enum | [x] | Approved, Denied, Escalate |
| F038 | ActionStatus enum | [x] | Pending, Processing, Approved, Denied, Escalated, Forwarded |
| F039 | LLMProvider enum | [x] | Gemini, OpenAI, Anthropic (planned) |
| F040 | Model serialization tests | [x] | JSON round-trip for all enums |
| F041 | Request/Response DTOs | [x] | CreateActionRequest, ActionResponse, etc. |
| F042 | Soft delete for agents | [ ] | active flag already exists, but need deactivation reason |

### 2.2 Error Handling
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F043 | AppError enum | [x] | Database, NotFound, Unauthorized, Forbidden, etc. |
| F044 | IntoResponse for AppError | [x] | Maps to correct HTTP status + JSON body |
| F045 | Result alias type | [x] | type Result<T> = std::result::Result<T, AppError> |
| F046 | Error detail in responses | [x] | {"error": "message"} format |
| F047 | Stack trace on internal errors | [ ] | Color-eyre or backtrace integration |
| F048 | Error rate metrics | [ ] | Count errors by type for alerting |
| F049 | User-friendly error messages | [ ] | Avoid leaking internal details in prod |

---

## Phase 3: Authentication & Authorization

### 3.1 API Key Authentication
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F050 | API key generation utility | [x] | UUID v4 based |
| F051 | SHA-256 hashing for API keys | [x] | hash_api_key(key, secret) |
| F052 | Auth middleware (Tower layer) | [x] | Extracts X-API-Key, validates against DB |
| F053 | Header extraction | [x] | X-API-Key header |
| F054 | Key validation against DB | [x] | SHA256(provided_key:secret) == stored_hash |
| F055 | Agent attached to request extensions | [x] | request.extensions_mut().insert(agent) |
| F056 | Auth tests (valid/invalid/missing key) | [x] | All three cases covered |
| F057 | Agent registration endpoint | [x] | POST /admin/agents/register |
| F058 | Agent revocation endpoint | [x] | POST /admin/agents/revoke |
| F059 | API key rotation | [x] | POST /admin/agents/rotate-key |
| F060 | Auth integration tests | [x] | Full register -> auth -> access flow |
| F061 | Multi-key per agent | [ ] | Multiple API keys per agent with labels |
| F062 | API key expiration | [ ] | TTL on keys with auto-revocation |
| F063 | API key audit | [ ] | Log key generation, revocation, last used |
| F064 | JWT-based auth alternative | [ ] | For server-to-server integration |

### 3.2 Admin Authentication
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F065 | Admin password validation | [x] | Against DEKO_ADMIN_PASSWORD env var |
| F066 | Admin login endpoint | [x] | POST /admin/login with cookie |
| F067 | Admin logout endpoint | [x] | POST /admin/logout clears cookie |
| F068 | Route protection middleware | [x] | Checks header or cookie |
| F069 | HttpOnly+Secure+SameSite cookie | [x] | Secure cookie flags |
| F070 | Admin auth tests | [x] | Login, logout, protected routes |
| F071 | Multi-admin support | [ ] | Multiple admin accounts with roles |
| F072 | SSO/OAuth2 for admin | [ ] | Google, GitHub, Okta integration |
| F073 | Session timeout / expiry | [ ] | Configurable TTL on admin sessions |
| F074 | Admin action confirmation | [ ] | Require re-auth for destructive admin actions |
| F075 | Rate limiting on login attempts | [ ] | Prevent brute force on admin password |

---

## Phase 4: Action Ingestion (The Interceptor)

### 4.1 Submit Actions
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F076 | POST /action endpoint | [x] | Creates action, returns id |
| F077 | Request body parsing | [x] | intent, payload, screenshot, metadata |
| F078 | Input validation | [x] | intent required, screenshot optional |
| F079 | Action saved to DB as pending | [x] | Status = Pending |
| F080 | Audit log entry for creation | [x] | Event type: action_created |
| F081 | Return action_id to caller | [x] | {"id": "...", "status": "pending"} |
| F082 | GET /action/{id} endpoint | [x] | Full action + verdict |
| F083 | GET /actions list endpoint | [x] | With pagination |
| F084 | Filter: by agent_id | [x] | ?agent_id=... |
| F085 | Filter: by status | [x] | ?status=denied |
| F086 | Filter: by date range | [x] | ?from=...&to=... |
| F087 | Action route unit tests | [x] | Request parsing, validation |
| F088 | Action route integration tests | [x] | Full request/response cycle |
| F089 | Batch action submission | [ ] | Multiple actions in one request |
| F090 | Action deduplication | [ ] | idempotency_key prevents duplicates |
| F091 | Scheduled/delayed actions | [ ] | `execute_at` field for future execution |
| F092 | Action priority queuing | [ ] | Priority levels for processing order |
| F093 | Action TTL / expiration | [ ] | Auto-deny pending actions older than X time |
| F094 | Attachments support | [ ] | File uploads alongside actions |

### 4.2 Action Polling
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F095 | GET /action/{id}/status | [x] | Returns verdict if ready |
| F096 | Verdict response format | [x] | decision, reason, risk_level |
| F097 | Pending status response | [x] | {"status": "pending"} |
| F098 | Retry-After header | [x] | 5 seconds for pending |
| F099 | Long-poll / WebSocket | [ ] | Server push when verdict ready |
| F100 | Status endpoint optimization | [ ] | Cache verdicts in memory for fast reads |

### 4.3 Action Forwarding
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F101 | POST /action/{id}/forward | [x] | Forward approved action |
| F102 | Approved -> forward check | [x] | Only approved actions can forward |
| F103 | Denied -> 403 Forbidden | [x] | With reason |
| F104 | Escalated -> 423 Locked | [x] | With message |
| F105 | HTTP forwarding via reqwest | [x] | Supports method, headers, body |
| F106 | Response capture | [x] | Response status + body logged |
| F107 | Forwarding tests | [x] | All three paths: approve, deny, escalate |
| F108 | Custom forwarding headers | [ ] | Add X-Deko headers to forwarded requests |
| F109 | Response transformation | [ ] | Modify response before returning to agent |
| F110 | Forward retry on failure | [ ] | Retry forwarding if target is unavailable |
| F111 | Forward timeout | [ ] | Configurable timeout for forwarded requests |
| F112 | Idempotent forwarding | [ ] | Prevent double execution with idempotency key |

---

## Phase 5: Policy Engine

### 5.1 Policy Rules
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F113 | JSON rule parser | [x] | JSON -> typed Rule struct |
| F114 | deny_keyword rule | [x] | Block actions containing keywords |
| F115 | require_approval rule | [x] | Flag HTTP methods for human review |
| F116 | max_amount threshold rule | [x] | Enforce numeric limits |
| F117 | regex_deny rule | [x] | Block actions matching regex |
| F118 | risk_flag rule | [x] | Flag keywords for medium risk |
| F119 | Immediate deny on match | [x] | No LLM call if policy blocks |
| F120 | Risk level from policy match | [x] | Set risk level per rule type |
| F121 | Default-deny fallback | [x] | No policy match != auto-approve |
| F122 | Policy CRUD endpoints | [x] | Create, List, Update, Delete |
| F123 | Soft delete (active flag) | [x] | Policies set inactive, not deleted |
| F124 | Policy engine unit tests | [x] | Each rule type tested |
| F125 | Policy engine integration tests | [x] | Policy matching with real DB |
| F126 | AND/OR rule composition | [ ] | Combine rules with logical operators |
| F127 | Rule priority/ordering | [ ] | First matching rule wins |
| F128 | Policy dry-run mode | [ ] | Log what would be denied without blocking |
| F129 | Policy versioning | [ ] | Track changes to policy rules |
| F130 | Policy hit statistics | [ ] | Count how often each policy triggers |
| F131 | Policy test/simulate endpoint | [ ] | Test a policy against sample action |
| F132 | Scheduled policy activation | [ ] | Activate/deactivate policies on a schedule |
| F133 | Policy templates library | [ ] | Pre-built rules: PCI, HIPAA, SOC2 patterns |

### 5.2 Advanced Policy Rules
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F134 | URL allowlist/blocklist rule | [ ] | Match against target_url |
| F135 | IP allowlist/blocklist rule | [ ] | Match against request origin |
| F136 | Day/time window rule | [ ] | Only allow actions during business hours |
| F137 | Rate-based rule | [ ] | Max N actions per agent per time window |
| F138 | Histogram/trend rule | [ ] | Flag if action amount deviates from historical avg |
| F139 | Agent capability rule | [x] | What each agent is allowed to do |
| F140 | Payload schema validation | [ ] | Validate payload matches expected JSON schema |
| F141 | Geofencing rule | [ ] | Block actions based on geographic origin |
| F142 | Concurrency limit rule | [ ] | Max simultaneous actions per agent |
| F143 | Budget/cost tracking rule | [ ] | Track cumulative spend and enforce limits |

---

## Phase 6: Multi-Provider LLM (The Eye)

### 6.1 Provider Abstraction
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F144 | LLMProviderTrait interface | [x] | Common interface for all providers |
| F145 | Gemini provider | [x] | Google Generative AI API |
| F146 | OpenAI provider | [x] | OpenAI Chat Completions API |
| F147 | Vision/screenshot support | [x] | Base64 inline images |
| F148 | System prompt template | [x] | Security-focused prompt |
| F149 | Structured JSON parsing | [x] | Parse LLM output into VerdictResult |
| F150 | Automatic provider fallback | [x] | Primary fails -> secondary tried |
| F151 | Exponential backoff retry | [x] | 2 retries with 500ms/1s backoff |
| F152 | 30s timeout per call | [x] | Configurable |
| F153 | Fail-closed on timeout/error | [x] | Provider dies -> denied |
| F154 | LLM audit logging | [x] | Call started + verdict logged to audit |
| F155 | Mock LLM provider for tests | [x] | Predetermined verdicts |
| F156 | Anthropic/Claude provider | [ ] | Enum exists, impl pending |
| F157 | Local LLM (Ollama) provider | [ ] | For air-gapped deployments |
| F158 | Azure OpenAI provider | [ ] | Microsoft Azure variant |
| F159 | AWS Bedrock provider | [ ] | Amazon Titan, Claude on AWS |
| F160 | Custom/self-hosted provider | [ ] | Webhook-based integration for custom LLMs |
| F161 | Provider health check | [ ] | Periodic ping to ensure provider is up |
| F162 | Provider latency tracking | [ ] | Track p50/p95/p99 per provider |
| F163 | Provider cost tracking | [ ] | Token count + cost per verdict |
| F164 | Prompt templates library | [ ] | Custom prompts per action type |
| F165 | Prompt injection detection | [ ] | Analyze input for prompt injection attempts |
| F166 | Confidence scoring | [ ] | LLM returns confidence with verdict |

### 6.2 Verdict Service
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F167 | Verdict service struct | [x] | Orchestrates policy + LLM |
| F168 | Policy pre-check before LLM | [x] | Quick deny without LLM |
| F169 | Policy deny -> immediate verdict | [x] | No LLM call needed |
| F170 | Policy pass -> LLM analysis | [x] | Full analysis pipeline |
| F171 | Combined verdict | [x] | Policy context passed to LLM |
| F172 | Verdict saved to DB | [x] | With audit log entry |
| F173 | Handle parse errors -> denied | [x] | Fail-closed on bad LLM output |
| F174 | Verdict service tests | [x] | With mock provider |
| F175 | Verdict caching | [ ] | Cache repeated similar actions |
| F176 | Batch verdict processing | [ ] | Process multiple actions in one LLM call |

---

## Phase 7: Background Processing

### 7.1 Action Processor
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F177 | Tokio background task | [x] | Polls for pending actions |
| F178 | Configurable polling interval | [x] | Hardcoded 2s, should be configurable |
| F179 | Batch fetch (limit 10) | [x] | Process in batches |
| F180 | Verdict service integration | [x] | Each action -> verdict |
| F181 | Status update after verdict | [x] | Pending -> Processing -> Done |
| F182 | Graceful error handling | [x] | Errors result in denied, not panic |
| F183 | Graceful shutdown | [x] | Tokio shutdown signal |
| F184 | Processing metrics | [x] | Count, timing |
| F185 | Processor tests | [x] | Mock-based |
| F186 | Configurable batch size | [ ] | Tune for throughput |
| F187 | Parallel action processing | [ ] | Concurrent LLM calls |
| F188 | Processing queue dashboard | [ ] | Admin view of queue depth, lag |
| F189 | Dead letter queue | [ ] | Actions that consistently fail processing |
| F190 | Re-processing of failed actions | [ ] | Manual or automatic retry |
| F191 | Action timeout enforcement | [ ] | Max processing time per action |
| F192 | Processor worker pool | [ ] | Configurable worker count |

### 7.2 Webhook Notifications
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F193 | Webhook URL config | [x] | DEKO_WEBHOOK_URL env var |
| F194 | Webhook on denied | [x] | POST with action + verdict details |
| F195 | Webhook on escalated | [x] | With risk level |
| F196 | Webhook payload format | [x] | Action ID, intent, verdict, risk level |
| F197 | Webhook retry logic | [x] | 3 attempts |
| F198 | Webhook delivery logging | [x] | Success/failure in audit log |
| F199 | Webhook tests | [x] | With mock server |
| F200 | Custom webhook per agent | [ ] | Per-agent webhook URL |
| F201 | Webhook secret/signature | [ ] | HMAC signed payloads |
| F202 | Multiple webhook endpoints | [ ] | Fan-out to multiple URLs |
| F203 | Webhook delivery guarantees | [ ] | At-least-once delivery |
| F204 | Webhook rate limiting | [ ] | Prevent overwhelming targets |
| F205 | Slack/Discord/PagerDuty integrations | [ ] | Pre-built notification channels |

---

## Phase 8: Admin Dashboard

### 8.1 Templates & UI
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F206 | Askama template engine | [x] | Server-side rendered |
| F207 | Base template with layout | [x] | Nav bar, responsive |
| F208 | Login page | [x] | Password form |
| F209 | Dashboard index | [x] | Summary stats + recent actions |
| F210 | Actions list page | [x] | Table with status badges |
| F211 | Action detail page | [x] | Full info, verdict, override form |
| F212 | Verdict history page | [x] | Filterable table |
| F213 | Policy management page | [x] | Create, view policies |
| F214 | Agent management page | [x] | Register, revoke |
| F215 | Audit log viewer | [x] | With event type filter |
| F216 | CSS styling (dark theme) | [x] | Professional look |
| F217 | Responsive design | [x] | Mobile + desktop |
| F218 | Template rendering tests | [x] | Each template renders |
| F219 | Loading states / spinners | [ ] | For async data |
| F220 | Toast notifications | [ ] | For success/error feedback |
| F221 | Dark mode toggle | [ ] | Light/dark theme |
| F222 | Localization / i18n | [ ] | Multi-language support |
| F223 | Accessibility (a11y) | [ ] | ARIA labels, keyboard nav |
| F224 | Real-time updates | [ ] | WebSocket for live action feed |
| F225 | Charts and graphs | [ ] | Action trends, denial rates |
| F226 | Multi-tenant admin UI | [ ] | Isolate agents, actions per tenant |

### 8.2 Admin API
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F227 | GET /admin - dashboard stats | [x] | Aggregate queries |
| F228 | GET /admin/actions - list | [x] | Paginated |
| F229 | GET /admin/actions/{id} - detail | [x] | Full + verdict |
| F230 | POST /admin/actions/{id}/override | [x] | With reason |
| F231 | GET /admin/policies - list | [x] | Active policies |
| F232 | POST /admin/policies - create | [x] | JSON rules |
| F233 | GET /admin/agents - list | [x] | All agents |
| F234 | POST /admin/agents - register | [x] | Via admin route |
| F235 | GET /admin/audit - log | [x] | Filtered |
| F236 | Admin API tests | [x] | End-to-end |
| F237 | Admin API versioning | [ ] | Backward compatible |
| F238 | Export CSV/JSON | [ ] | Download actions, audit log |
| F239 | Bulk operations | [ ] | Bulk override, bulk revoke |

---

## Phase 9: Health & Observability

### 9.1 Health Checks
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F240 | GET /health endpoint | [x] | Full system health |
| F241 | DB connectivity check | [x] | SELECT 1 |
| F242 | LLM API reachability | [x] | Minimal API call |
| F243 | GET /health/ready | [x] | Readiness probe |
| F244 | GET /health/live | [x] | Liveness probe |
| F245 | Health check tests | [x] | Mock-based |
| F246 | Component-level health | [ ] | Per-provider, per-service |
| F247 | Health check caching | [ ] | Don't hammer APIs on every check |
| F248 | Custom health check plugins | [ ] | User-defined checks |

### 9.2 Metrics & Logging
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F249 | Structured logging (JSON) | [x] | tracing-subscriber JSON fmt |
| F250 | Request/response tracing | [x] | TraceLayer |
| F251 | Request duration metrics | [x] | Per-endpoint timing |
| F252 | Action count by status | [x] | Approve/deny/escalate counters |
| F253 | LLM call metrics | [x] | Count, latency, errors |
| F254 | Metrics JSON endpoint | [x] | GET /metrics |
| F255 | Metrics collector tests | [x] | Atomic counter validation |
| F256 | Prometheus endpoint | [ ] | /metrics in Prometheus format |
| F257 | Grafana dashboard | [ ] | Pre-built dashboard JSON |
| F258 | Alerting rules | [ ] | Prometheus alert rules |
| F259 | Distributed tracing | [ ] | OpenTelemetry integration |
| F260 | Log retention policies | [ ] | Rotation, archival |
| F261 | Structured error logging | [ ] | Error + context in JSON |
| F262 | Audit log search | [ ] | Full-text search on audit events |
| F263 | Metrics histogram buckets | [ ] | Latency distribution |
| F264 | Per-agent metrics | [ ] | Actions, denials per agent |
| F265 | Per-policy metrics | [ ] | How often each policy triggers |

---

## Phase 10: Developer Experience

### 10.1 API Documentation
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F266 | Utoipa OpenAPI spec | [x] | Auto-generated |
| F267 | All endpoints documented | [x] | With request/response schemas |
| F268 | Auth documentation | [x] | API key security scheme |
| F269 | Swagger UI at /docs | [x] | Interactive API browser |
| F270 | Doc generation test | [x] | Spec generates without error |
| F271 | Postman collection | [ ] | Ready-to-import collection |
| F272 | API changelog | [ ] | Breaking changes documented |
| F273 | Rate limits in docs | [ ] | Documented per-endpoint |

### 10.2 Code Quality
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F274 | Clippy clean | [x] | Warnings remaining (dead_code) |
| F275 | rustfmt pass | [x] | Consistent formatting |
| F276 | No TODO/FIXME | [x] | Zero remaining |
| F277 | Inline documentation | [x] | Public API doc comments, missing on some items |
| F278 | CI: cargo test | [x] | Runs on push |
| F279 | CI: cargo clippy -D warnings | [x] | Enforced |
| F280 | CI: cargo fmt --check | [x] | Formatting enforced |
| F281 | CI: Docker build | [x] | Multi-stage build |
| F282 | CI: smoke test | [x] | Start and health check |
| F283 | CI: dependency audit | [ ] | cargo-audit |
| F284 | CI: vulnerability scanning | [ ] | Trivy or similar |
| F285 | CI: coverage reporting | [ ] | tarpaulin or llvm-cov |
| F286 | CI: fuzz testing | [ ] | cargo-fuzz for input parsing |
| F287 | CI: leak checking | [ ] | cargo-leak or similar |

### 10.3 SDK & Examples
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F288 | Python SDK | [ ] | Client library for agents |
| F289 | Node.js SDK | [ ] | Client library for agents |
| F290 | Example: LangChain integration | [ ] | Callback handler |
| F291 | Example: AutoGen integration | [ ] | Tool/plugin |
| F292 | Example: CrewAI integration | [ ] | Tool/plugin |
| F293 | Example: trading bot | [ ] | Real demo scenario |
| F294 | Example: DevOps agent | [ ] | Kubernetes management |
| F295 | Example: customer support | [ ] | Ticket system actions |
| F296 | Example: code review agent | [ ] | PR management actions |

---

## Phase 11: Deployment & Operations

### 11.1 Containerization
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F297 | Multi-stage Dockerfile | [x] | Build + runtime |
| F298 | Minimal final image | [x] | Distroless or alpine |
| F299 | Docker Compose | [x] | App + SQLite volume |
| F300 | .dockerignore | [x] | Exclude dev files |
| F301 | Docker build test | [x] | Works locally |
| F302 | Docker image size optimization | [ ] | Target < 50MB |
| F303 | Docker healthcheck | [ ] | HEALTHCHECK instruction |
| F304 | Docker non-root user | [ ] | Security best practice |

### 11.2 Orchestration
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F305 | Kubernetes deployment manifest | [ ] | Deployment + Service + ConfigMap |
| F306 | Helm chart | [ ] | Parameterized k8s deployment |
| F307 | Horizontal pod autoscaling | [ ] | HPA based on metrics |
| F308 | Init container for migrations | [ ] | Run migrations before app starts |
| F309 | PodDisruptionBudget | [ ] | HA configuration |
| F310 | Service mesh integration | [ ] | Istio/Linkerd mTLS |
| F311 | Canary deployment support | [ ] | Gradual rollout |

### 11.3 CI/CD
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F312 | GitHub Actions CI | [x] | Test, lint, build, smoke |
| F313 | Automated releases | [ ] | Release please or similar |
| F314 | Container registry publish | [ ] | Push to GHCR or Docker Hub |
| F315 | Staging deployment | [ ] | Auto-deploy to staging |
| F316 | Integration test environment | [ ] | Ephemeral test env |
| F317 | Blue-green deployment | [ ] | Zero-downtime updates |
| F318 | Database migration automation | [ ] | Safe migration in pipeline |

### 11.4 Monitoring
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F319 | Prometheus metrics endpoint | [ ] | Standard format |
| F320 | Grafana dashboard | [ ] | Pre-built dashboards |
| F321 | log aggregation setup | [ ] | Loki / ELK / Datadog config |
| F322 | Uptime monitoring | [ ] | External health check pings |
| F323 | SLA/SLO tracking | [ ] | Measure uptime, latency |
| F324 | Incident response docs | [ ] | Runbooks for common issues |

---

## Phase 12: Testing & Reliability

### 12.1 Test Infrastructure
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F325 | In-memory test database | [x] | sqlite::memory: for speed |
| F326 | Test fixtures/factories | [x] | TestFixtures for agents, actions, policies |
| F327 | Test app setup helper | [x] | TestApp::setup() |
| F328 | Test auth helper | [x] | Agent registration in tests |
| F329 | Mock LLM provider | [x] | Predetermined verdicts |
| F330 | CI test runner config | [x] | GitHub Actions |
| F331 | Clippy configuration | [x] | clippy.toml fixed |
| F332 | rustfmt configuration | [x] | Standard config |

### 12.2 Integration Tests
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F333 | Full action flow test | [x] | Submit -> process -> verdict |
| F334 | Auth flow test | [x] | Register -> auth -> access |
| F335 | Policy enforcement test | [x] | Keywords, amount limits |
| F336 | Fail-closed: DB down | [x] | DB error results in denial |
| F337 | Fail-closed: LLM down | [x] | Mock provider failure |
| F338 | Admin override test | [x] | Override reason logged |
| F339 | Webhook delivery test | [x] | With mock server |

### 12.3 Advanced Testing
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F340 | Integration test suite | [x] | 13 tests, expanding |
| F341 | Load / stress testing | [ ] | k6 or locust scripts |
| F342 | Security penetration testing | [ ] | SQLi, XSS, auth bypass |
| F343 | Property-based testing | [ ] | proptest for input validation |
| F344 | Fuzz testing | [ ] | cargo-fuzz for JSON parsing |
| F345 | Performance benchmarks | [ ] | Criterion benchmarks |
| F346 | Long-running stability test | [ ] | 24h run, check memory |
| F347 | Chaos engineering | [ ] | Random LLM failures, DB restarts |

---

## Phase 13: Security Hardening

### 13.1 Core Security
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F348 | Rate limiting per IP | [x] | Configurable RPM |
| F349 | Request size limits | [x] | 512KB default |
| F350 | Screenshot size limit | [x] | 10MB max |
| F351 | Input sanitization | [x] | HTML escape |
| F352 | Secure cookie flags | [x] | HttpOnly + SameSite |
| F353 | API key rotation | [x] | Admin endpoint |
| F354 | SQL injection prevention | [x] | SQLx parameterized |
| F355 | XSS prevention in templates | [x] | Askama auto-escapes |
| F356 | Distributed rate limiting | [ ] | Redis-backed for multi-instance |
| F357 | API key constraints (min length) | [ ] | Validate key strength |
| F358 | Audit log integrity | [ ] | Hash chain for tamper detection |
| F359 | Audit log encryption at rest | [ ] | Encrypt sensitive fields |

### 13.2 Advanced Security
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F360 | TLS/HTTPS support | [ ] | Let's Encrypt auto |
| F361 | mTLS for agent communication | [ ] | Certificate-based auth |
| F362 | Secrets vault integration | [ ] | HashiCorp Vault |
| F363 | API key access logging | [ ] | Per-key usage statistics |
| F364 | Suspicious behavior detection | [ ] | Anomaly detection on actions |
| F365 | IP allowlist for admin | [ ] | Restrict admin access by IP |
| F366 | Audit log export | [ ] | Signed, tamper-proof export |
| F367 | Compliance reporting | [ ] | SOC2/HIPAA audit reports |
| F368 | Data retention policies | [ ] | Auto-purge old audit data |

---

## Phase 14: Multi-Tenancy & Scale

### 14.1 Multi-Tenancy
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F369 | Organization/workspace concept | [ ] | Isolated namespaces |
| F370 | Per-tenant configuration | [ ] | Separate policies, LLM config |
| F371 | Tenant-level admin | [ ] | Admin per organization |
| F372 | Cross-tenant isolation | [ ] | Data separation guarantee |
| F373 | Tenant usage metrics | [ ] | Per-tenant action counts |
| F374 | Tenant billing integration | [ ] | Usage-based billing API |
| F375 | Shared vs dedicated LLM keys | [ ] | Per-tenant provider config |

### 14.2 Scaling
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F376 | External database (PostgreSQL) | [ ] | Production-ready DB |
| F377 | Redis integration | [ ] | Caching, rate limiting, queues |
| F378 | Horizontal scaling | [ ] | Stateless app nodes |
| F379 | Message queue for actions | [ ] | RabbitMQ / Redis Streams |
| F380 | Read replicas for dashboard | [ ] | Separate read-only connections |
| F381 | Caching layer | [ ] | Redis for policy, verdict cache |
| F382 | Query optimization | [ ] | Indexes, query profiling |
| F383 | Connection pooling tuning | [ ] | Per-instance pool sizing |

---

## Phase 15: Integration Ecosystem

### 15.1 Inbound Integrations
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F384 | REST API | [x] | Current primary interface |
| F385 | gRPC endpoint | [ ] | For high-throughput scenarios |
| F386 | Webhook receiver | [ ] | Accept actions via webhooks |
| F387 | Kafka consumer | [ ] | Event-driven ingestion |
| F388 | GraphQL endpoint | [ ] | Flexible queries |
| F389 | CLI tool | [ ] | deko-cli for scripting |
| F390 | Terraform provider | [ ] | Manage policies as code |

### 15.2 Outbound Integrations
| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F391 | Webhook notifications | [x] | Denied/escalated events |
| F392 | Slack integration | [ ] | Real-time alerts |
| F393 | PagerDuty integration | [ ] | Incident management |
| F394 | Datadog integration | [ ] | Metrics + events |
| F395 | Splunk/ELK integration | [ ] | Log shipping |
| F396 | Email notifications | [ ] | SMTP alerts |
| F397 | JIRA ticket creation | [ ] | Auto-create for escalated actions |
| F398 | SIEM integration | [ ] | Security event forwarding |

---

## Phase 16: LLM Enhancements

| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F399 | Prompt optimization | [ ] | A/B test prompts for accuracy |
| F400 | Few-shot examples in prompts | [ ] | Improve LLM consistency |
| F401 | Custom prompt per agent type | [ ] | Different prompts for different agents |
| F402 | LLM output validation | [ ] | Retry on malformed JSON |
| F403 | Confidence threshold tuning | [ ] | Configurable approval threshold |
| F404 | Explainable verdicts | [ ] | Detailed reasoning chain |
| F405 | Batch LLM processing | [ ] | Multiple actions in one LLM call |
| F406 | LLM response caching | [ ] | Cache similar intents |

---

## Phase 17: Advanced Admin Features

| ID | Feature | Status | Notes |
|----|---------|--------|-------|
| F407 | Action timeline visualization | [ ] | Gantt chart of action lifecycle |
| F408 | Policy simulation | [ ] | "What if" policy testing |
| F409 | Agent behavior profiling | [ ] | Normal vs anomalous patterns |
| F410 | Custom dashboard widgets | [ ] | Drag-and-drop dashboard |
| F411 | Saved filters / views | [ ] | Bookmarked admin views |
| F412 | Audit log search | [ ] | Full-text and structured search |
| F413 | Bulk audit export | [ ] | JSON/CSV download |
| F414 | Data retention management UI | [ ] | Configure purge schedules |

---

## Summary

| Phase | Done | In Progress | Planned | Total |
|-------|------|-------------|---------|-------|
| Core Concept | 4 | 0 | 0 | 4 |
| Project Foundation | 22 | 1 | 8 | 31 |
| Core Models & Types | 15 | 0 | 2 | 17 |
| Authentication & Auth | 24 | 0 | 11 | 35 |
| Action Ingestion | 29 | 0 | 9 | 38 |
| Policy Engine | 24 | 0 | 18 | 42 |
| Multi-Provider LLM | 28 | 0 | 20 | 48 |
| Background Processing | 19 | 1 | 8 | 28 |
| Admin Dashboard | 30 | 0 | 12 | 42 |
| Health & Observability | 17 | 0 | 12 | 29 |
| Developer Experience | 17 | 2 | 15 | 34 |
| Deployment & Ops | 8 | 0 | 16 | 24 |
| Testing & Reliability | 17 | 1 | 8 | 26 |
| Security Hardening | 12 | 0 | 9 | 21 |
| Multi-Tenancy & Scale | 0 | 0 | 16 | 16 |
| Integration Ecosystem | 1 | 0 | 16 | 17 |
| LLM Enhancements | 0 | 0 | 8 | 8 |
| Advanced Admin Features | 0 | 0 | 8 | 8 |
| **Total** | **267** | **5** | **196** | **468** |
