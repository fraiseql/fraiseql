# Basic two-subgraph federation example.
#
# Prerequisites: docker + docker compose, and the Apollo `rover` CLI:
# https://www.apollographql.com/docs/rover/getting-started
#
#   make run     # boot both subgraphs, compose the supergraph, start the router
#   make demo    # query through the router on :4000
#   make down    # tear everything down

ROVER ?= rover

.PHONY: build supergraph run demo demo-subgraphs down clean

## Build both subgraph images and bring them up with their databases.
##
## --wait matters: plain `up -d` returns once the containers are CREATED, so the
## `supergraph` step below would race their boot and rover would get a connection
## refused. Both subgraph images declare a HEALTHCHECK, so --wait blocks until they
## actually answer.
build:
	docker compose build
	docker compose up -d --wait postgres-users postgres-orders users-service orders-service

## Compose the supergraph schema from the running subgraphs (needs rover).
##
## Written to a temporary file and moved into place only on success: `> …graphql`
## truncates the target before rover runs, so a failed composition would destroy the
## schema the router is currently serving.
supergraph:
	@$(ROVER) supergraph compose --config router/supergraph.yaml > router/supergraph.graphql.tmp \
	  || { rm -f router/supergraph.graphql.tmp; exit 1; }
	@mv router/supergraph.graphql.tmp router/supergraph.graphql
	@echo "Wrote router/supergraph.graphql"

## Full stack: subgraphs up, supergraph composed, router serving on :4000.
run: build supergraph
	docker compose up -d router
	@echo "Router ready at http://localhost:4000/graphql"

## One federated query: a user, with the orders that live in the other database.
demo:
	@curl -s -X POST http://localhost:4000/graphql \
	  -H 'Content-Type: application/json' \
	  -d '{"query":"{ users { id name orders { id status total } } }"}'
	@echo

## The same data straight from each subgraph, without the router.
demo-subgraphs:
	@echo "users-service:"
	@curl -s -X POST http://localhost:4001/graphql -H 'Content-Type: application/json' \
	  -d '{"query":"{ users { id name email } }"}'; echo
	@echo "orders-service:"
	@curl -s -X POST http://localhost:4002/graphql -H 'Content-Type: application/json' \
	  -d '{"query":"{ orders { id status total } }"}'; echo

## Tear everything down.
down:
	docker compose down -v

## Remove the generated supergraph schema.
clean: down
	rm -f router/supergraph.graphql router/supergraph.graphql.tmp
