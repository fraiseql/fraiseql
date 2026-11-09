#!/bin/bash
# Create first-party auth's role (#1965).
#
# First-party auth runs on a role of its own (`[auth.database]`): it creates schema
# `auth`, owns every table in it, and has no superuser or BYPASSRLS attribute. This
# script creates that role when it is absent, sets its password, and lets it connect to
# and create in the database. It does not create schema `auth`: the auth DDL creates and
# marks it, and refuses a schema `auth` it did not create.
#
# Idempotent: a role that exists keeps its attributes and gets only its password set, so
# it runs on every `docker compose up`, against a volume that already holds the role.
#
# Owned once and run by three things, so none can drift from the others:
#   * `docker/docker-compose.test.yml` and `pgService` in `.dagger/main.go`, as an initdb
#     script, like `postgres-replication-init.sh`: a two-role layout that existed on only
#     one test rig would make the auth suites green on one and red on the other. Suites
#     that create databases of their own grant the role on them;
#   * the `auth-role` service of the canonical `docker-compose.yml`, against the
#     `postgres` service (PGHOST and PGPASSWORD set).
set -euo pipefail

AUTH_ROLE="${AUTH_ROLE:-fraiseql_auth}"
AUTH_PASSWORD="${AUTH_PASSWORD:-fraiseql_auth_test_password}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    -v auth_role="$AUTH_ROLE" -v auth_password="$AUTH_PASSWORD" -v db="$POSTGRES_DB" <<-'EOSQL'
	SELECT format('CREATE ROLE %I WITH LOGIN NOSUPERUSER NOBYPASSRLS', :'auth_role')
	 WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'auth_role') \gexec
	SELECT format('ALTER ROLE %I PASSWORD %L', :'auth_role', :'auth_password') \gexec
	GRANT CONNECT, CREATE ON DATABASE :"db" TO :"auth_role";
EOSQL
