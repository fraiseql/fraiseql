import http from 'k6/http';
import { check } from 'k6';

export const options = {
  vus: 1,
  iterations: 1,
};

export default function () {
  // Test FraiseQL
  console.log('Testing FraiseQL...');
  const fraiseqlRes = http.post(
    'http://benchmark-fraiseql:8000/graphql',
    JSON.stringify({ query: '{ __typename }' }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(fraiseqlRes, {
    'FraiseQL status 200': (r) => r.status === 200,
  });

  if (fraiseqlRes.status === 200) {
    console.log('FraiseQL response:', fraiseqlRes.body);
  } else {
    console.log('FraiseQL error:', fraiseqlRes.status, fraiseqlRes.body);
  }

  // Test Strawberry
  console.log('\nTesting Strawberry...');
  const strawberryRes = http.post(
    'http://benchmark-strawberry:8000/graphql',
    JSON.stringify({ query: '{ __typename }' }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(strawberryRes, {
    'Strawberry status 200': (r) => r.status === 200,
  });

  if (strawberryRes.status === 200) {
    console.log('Strawberry response:', strawberryRes.body);
  } else {
    console.log('Strawberry error:', strawberryRes.status, strawberryRes.body);
  }
}
