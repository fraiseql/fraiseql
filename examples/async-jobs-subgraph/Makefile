# Async-jobs federation example.
#
# Prerequisites: docker + docker compose, and (for full federation) the Apollo
# `rover` CLI: https://www.apollographql.com/docs/rover/getting-started
#
# Quick path (subgraph only, no docker, no rover):
#   make dev        # cargo run the async-jobs subgraph on :4001
#   make demo-local # enqueue a job against :4001 and poll it
#
# Full federation path (router composing both subgraphs):
#   make run        # boot postgres + both subgraphs + router
#   make demo       # enqueue + poll through the router on :4000
#   make down       # tear everything down

ROVER ?= rover

.PHONY: dev demo-local build supergraph run demo down clean

## Run only the async-jobs subgraph locally (no docker).
dev:
	cd subgraph && PORT=4001 cargo run

## Hit the standalone subgraph directly on :4001.
demo-local:
	@echo "Enqueuing job..."
	@ID=$$(curl -s -X POST http://localhost:4001/graphql \
	  -H 'Content-Type: application/json' \
	  -d '{"query":"mutation { enqueueJob(input: \"hello\") { id status } }"}' \
	  | sed -n 's/.*"id":"\([^"]*\)".*/\1/p'); \
	echo "job id: $$ID"; \
	echo "polling (waiting for SUCCEEDED)..."; \
	sleep 3; \
	curl -s -X POST http://localhost:4001/graphql \
	  -H 'Content-Type: application/json' \
	  -d "{\"query\":\"query { jobStatus(id: \\\"$$ID\\\") { id status result } }\"}"; \
	echo

## Build subgraph images and bring up postgres + subgraphs (no router yet).
##
## --wait matters: plain `up -d` returns once the containers are CREATED, so the
## `supergraph` step below would race their boot and rover would get a connection
## refused. Both subgraph images declare a HEALTHCHECK, so --wait blocks until
## they actually answer.
build:
	docker compose build
	docker compose up -d --wait postgres subgraph fraiseql

## Compose the supergraph schema from the running subgraphs (needs rover).
##
## rover runs on the host and fetches each subgraph over its published port — see
## the comment in router/supergraph.yaml for why subgraph_url and routing_url
## differ. Compose into a temporary file and move it into place only on success:
## `> router/supergraph.graphql` truncates the target before rover even runs, so
## a failed composition destroys the schema the router is currently serving.
supergraph:
	@$(ROVER) supergraph compose --config router/supergraph.yaml > router/supergraph.graphql.tmp \
	  || { rm -f router/supergraph.graphql.tmp; exit 1; }
	@mv router/supergraph.graphql.tmp router/supergraph.graphql
	@echo "Wrote router/supergraph.graphql"

## Full stack: subgraphs up, supergraph composed, router serving on :4000.
run: build supergraph
	docker compose up -d router
	@echo "Router ready at http://localhost:4000/graphql"

## Enqueue + poll through the federation router on :4000.
demo:
	@echo "Enqueuing job via router..."
	@ID=$$(curl -s -X POST http://localhost:4000/graphql \
	  -H 'Content-Type: application/json' \
	  -d '{"query":"mutation { enqueueJob(input: \"hello\") { id status } }"}' \
	  | sed -n 's/.*"id":"\([^"]*\)".*/\1/p'); \
	echo "job id: $$ID"; \
	sleep 3; \
	curl -s -X POST http://localhost:4000/graphql \
	  -H 'Content-Type: application/json' \
	  -d "{\"query\":\"query { jobStatus(id: \\\"$$ID\\\") { id status result } }\"}"; \
	echo

## Tear everything down.
down:
	docker compose down -v

## Remove the generated supergraph schema.
clean: down
	rm -f router/supergraph.graphql router/supergraph.graphql.tmp
