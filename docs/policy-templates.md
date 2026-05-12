# Deko Policy Templates

## 1. Block Destructive Commands
```json
[
  {"type": "deny_keyword", "keywords": ["rm -rf", "drop table", "delete all", "truncate"], "immediate_deny": true}
]
```

## 2. Financial Transfer Limits
```json
[
  {"type": "max_amount", "max": 10000, "immediate_deny": true},
  {"type": "require_approval", "action_types": ["POST", "PUT"]}
]
```

## 3. Business Hours Only
```json
[
  {"type": "time_window", "start_hour_utc": 8, "end_hour_utc": 18, "days": [1, 2, 3, 4, 5]}
]
```

## 4. URL Restriction
```json
[
  {"type": "url_allowlist", "patterns": ["api.trusted.com", "internal.service"]}
]
```

## 5. Rate Limiting
```json
[
  {"type": "rate_limit", "max_count": 100, "window_secs": 60}
]
```

## 6. PCI Compliance (No sensitive data in actions)
```json
[
  {"type": "deny_keyword", "keywords": ["credit card", "ssn", "password", "secret"], "immediate_deny": true},
  {"type": "risk_flag", "keywords": ["payment", "refund"]}
]
```

## 7. SOC2 Compliance (Approval required for changes)
```json
[
  {"type": "require_approval", "action_types": ["DELETE", "PUT", "PATCH"]},
  {"type": "risk_flag", "keywords": ["deployment", "configuration", "migration"]}
]
```
