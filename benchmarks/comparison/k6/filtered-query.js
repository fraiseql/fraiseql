import http from 'k6/http';
import { check } from 'k6';

export const options = {
  scenarios: {
    sustained_load: {
      executor: 'constant-arrival-rate',
      rate: 800,
      timeUnit: '1s',
      duration: '5m',
      preAllocatedVUs: 50,
      maxVUs: 200,
    },
  },
  thresholds: {
    http_req_duration: ['p(50)<15', 'p(95)<75', 'p(99)<250'],
    http_req_failed: ['rate<0.01'],
  },
};

const ENDPOINT = __ENV.ENDPOINT || 'http://localhost:8080/graphql';

const QUERY = JSON.stringify({
  query: '{ users(where: { name: { eq: "test" } }) { id name email } }',
});

export default function () {
  const res = http.post(ENDPOINT, QUERY, {
    headers: { 'Content-Type': 'application/json' },
  });

  check(res, {
    'status is 200': (r) => r.status === 200,
    'no errors': (r) => {
      const body = JSON.parse(r.body);
      return !body.errors || body.errors.length === 0;
    },
    'has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data !== null && body.data !== undefined;
    },
  });
}
