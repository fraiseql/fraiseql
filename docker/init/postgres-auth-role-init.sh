#!/bin/bash
# Create first-party auth's role on the test primary (#1965).
#
# First-party auth runs on a role of its own (`[auth.database]`): it creates schema
# `auth`, owns every table in it, and has no superuser or BYPASSRLS attribute. This
# script creates that role and lets it create in the test database. It does not
# create schema `auth`: the auth DDL creates and marks it, and refuses a schema `auth`
# it did not create.
#
# Owned once and mounted by BOTH rigs, `docker/docker-compose.test.yml` locally and
# `pgService` in `.dagger/main.go`, like `postgres-replication-init.sh`: a two-role
# layout that existed on only one rig would make the auth suites green on one and red
# on the other. Suites that create databases of their own grant the role on them.
set -euo pipefail

AUTH_ROLE="${AUTH_ROLE:-fraiseql_auth}"
AUTH_PASSWORD="${AUTH_PASSWORD:-fraiseql_auth_test_password}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    -v auth_role="$AUTH_ROLE" -v auth_password="$AUTH_PASSWORD" -v db="$POSTGRES_DB" <<-'EOSQL'
	CREATE ROLE :"auth_role" WITH LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD :'auth_password';
	GRANT CONNECT, CREATE ON DATABASE :"db" TO :"auth_role";
EOSQL
