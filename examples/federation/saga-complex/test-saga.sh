#!/usr/bin/env bash
# Exercise the travel-booking saga, both paths.
#
# A saga test that only books successfully tests a sequence, not a saga. This
# asserts the compensation path too: `decline-*` is refused by payment-service
# deterministically, and every step taken before it must be compensated, in
# reverse order.
#
# Every failure here exits non-zero. The previous version polled for the router
# and, on timeout, fell through to the first test anyway — so a stack that never
# started was reported as a query failure rather than as a stack that never
# started.
set -uo pipefail

ROUTER="${ROUTER_URL:-http://localhost:4000/graphql}"
DEADLINE="${DEADLINE_SECS:-90}"
fail=0

note() { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32mok\033[0m   %s\n' "$*"; }
bad()  { printf '  \033[31mFAIL\033[0m %s\n' "$*"; fail=1; }

gql() {
  curl -s -m 20 -X POST -H 'Content-Type: application/json' -d "$1" "$ROUTER"
}

note "Waiting for the router at $ROUTER (up to ${DEADLINE}s)"
deadline=$((SECONDS + DEADLINE))
until [ -n "$(gql '{"query":"{ __typename }"}' 2>/dev/null)" ]; do
  if [ "$SECONDS" -ge "$deadline" ]; then
    bad "the router never answered within ${DEADLINE}s — the stack did not start"
    echo "     try: docker compose logs apollo-router"
    exit 1
  fi
  sleep 2
done
ok "router is answering"

BOOK='mutation($userId:ID!,$flightId:ID!,$hotelId:ID!,$carId:ID!){
        bookTravel(userId:$userId,flightId:$flightId,hotelId:$hotelId,carId:$carId){
          bookingId status reason reservations{ type status } compensations } }'

request() {
  python3 -c '
import json, sys
print(json.dumps({"query": sys.argv[1],
                  "variables": {"userId": sys.argv[2], "flightId": "FL-9",
                                "hotelId": "H-1", "carId": "C-1"}}))' "$BOOK" "$1"
}

note "1. Read side: Query.flight is federated to flight-service"
out=$(gql '{"query":"query($id:ID!){ flight(id:$id){ id departure arrival } }","variables":{"id":"FL-9"}}')
if printf '%s' "$out" | grep -q '"departure"'; then ok "flight resolved"; else bad "flight did not resolve: $out"; fi

note "2. Happy path: every step succeeds, nothing is compensated"
out=$(gql "$(request u-42)")
status=$(printf '%s' "$out" | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["bookTravel"]["status"])' 2>/dev/null)
steps=$(printf '%s' "$out" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)["data"]["bookTravel"]["reservations"]))' 2>/dev/null)
comps=$(printf '%s' "$out" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)["data"]["bookTravel"]["compensations"]))' 2>/dev/null)
[ "$status" = "confirmed" ] && ok "status=confirmed" || bad "status=$status (expected confirmed) — $out"
[ "$steps" = "4" ]         && ok "4 steps recorded"  || bad "steps=$steps (expected 4)"
[ "$comps" = "0" ]         && ok "no compensations"  || bad "compensations=$comps (expected 0)"

note "3. Failure path: payment declines, earlier steps are compensated in reverse"
out=$(gql "$(request decline-7)")
status=$(printf '%s' "$out" | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["bookTravel"]["status"])' 2>/dev/null)
order=$(printf '%s' "$out" | python3 -c 'import sys,json
c=json.load(sys.stdin)["data"]["bookTravel"]["compensations"]
print(",".join(x.split(":")[0] for x in c))' 2>/dev/null)
[ "$status" = "rolled_back" ] && ok "status=rolled_back" || bad "status=$status (expected rolled_back) — $out"
[ "$order" = "car,hotel,flight" ] && ok "compensated in reverse: $order" \
  || bad "compensation order=$order (expected car,hotel,flight)"

note "4. The decline is deterministic, not incidental"
a=$(gql "$(request decline-1)" | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["bookTravel"]["status"])' 2>/dev/null)
b=$(gql "$(request u-99)"      | python3 -c 'import sys,json;print(json.load(sys.stdin)["data"]["bookTravel"]["status"])' 2>/dev/null)
[ "$a" = "rolled_back" ] && [ "$b" = "confirmed" ] \
  && ok "decline-1 rolls back and u-99 confirms — the userId is what decides" \
  || bad "decline-1=$a u-99=$b (expected rolled_back / confirmed)"

if [ "$fail" -eq 0 ]; then
  printf '\n\033[32mAll saga assertions passed.\033[0m\n'
else
  printf '\n\033[31mSaga assertions FAILED.\033[0m\n'
fi
exit "$fail"
