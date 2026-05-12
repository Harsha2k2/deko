# Deko Operations Guide

## Health Checks

- `/health/live` - Liveness probe (always returns 200 if process is alive)
- `/health/ready` - Readiness probe (returns 200 when DB is connected)
- `/health` - Full health check (DB + LLM, returns 503 if degraded)
- `/metrics` - JSON metrics
- `/metrics/prometheus` - Prometheus-format metrics

## Monitoring Setup

### Prometheus Configuration
```yaml
scrape_configs:
  - job_name: 'deko'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics/prometheus'
```

### Recommended Alerts
```yaml
groups:
  - name: deko
    rules:
      - alert: HighDenialRate
        expr: rate(deko_actions_denied[5m]) / rate(deko_actions_total[5m]) > 0.5
        for: 5m
        labels: { severity: warning }
      - alert: LLMErrors
        expr: rate(deko_errors_llm[5m]) > 0
        for: 2m
        labels: { severity: critical }
      - alert: DatabaseErrors
        expr: rate(deko_errors_database[5m]) > 0
        for: 1m
        labels: { severity: critical }
```

### SLA Targets
- API uptime: 99.9%
- Action processing: 95% within 10 seconds
- LLM verdict accuracy: >99% (monitor override rate)

## Incident Response

### LLM Provider Down
1. Check `/health` - LLM status will show "unhealthy"
2. Deko automatically falls back to secondary LLM provider
3. If both fail, actions are denied (fail-closed)
4. Resolution: Restore API key or switch default provider

### Database Full / Slow
1. Check `/metrics` for database errors
2. For SQLite: `VACUUM;` and check disk space
3. For PostgreSQL: check connection pool and slow queries
4. Deko degrades gracefully - denies actions when DB is unreachable

### Rate Limit Issues
1. Check `DEKO_RATE_LIMIT_PER_MINUTE` is appropriate for your traffic
2. Increase or disable rate limiting for trusted agents
3. Distributed deployments need Redis-backed rate limiting

### High Denial Rate
1. Check which policies are triggering (audit log)
2. Review LLM provider responses (audit log has raw responses)
3. Consider dry-run mode: set `DEKO_POLICY_DRY_RUN=1`
4. Tune policies or adjust LLM prompt
