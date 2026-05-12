import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  stages: [
    { duration: '30s', target: 10 },
    { duration: '1m', target: 50 },
    { duration: '30s', target: 0 },
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'],
    http_req_failed: ['rate<0.01'],
  },
};

const API_KEY = 'test-key';
const BASE_URL = 'http://localhost:8000';

export default function () {
  const payload = JSON.stringify({
    intent: `Load test action ${__VU}-${__ITER}`,
    metadata: { source: 'k6-load-test' },
  });

  const res = http.post(`${BASE_URL}/action`, payload, {
    headers: { 'Content-Type': 'application/json', 'X-API-Key': API_KEY },
  });

  check(res, {
    'status is 201': (r) => r.status === 201,
    'has action id': (r) => JSON.parse(r.body).id !== undefined,
  });

  const health = http.get(`${BASE_URL}/health/live`);
  check(health, { 'health check ok': (r) => r.status === 200 });

  sleep(0.5);
}
