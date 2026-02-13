#!/bin/bash
# FraiseQL Test Database Manager
# Simplifies common Docker Compose operations for testing

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
COMPOSE_FILE="$PROJECT_ROOT/docker-compose.yml"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
info() {
  echo -e "${BLUE}ℹ${NC} $1"
}

success() {
  echo -e "${GREEN}✓${NC} $1"
}

warn() {
  echo -e "${YELLOW}⚠${NC} $1"
}

error() {
  echo -e "${RED}✗${NC} $1"
  exit 1
}

# Check if docker is running
check_docker() {
  if ! command -v docker &> /dev/null; then
    error "Docker is not installed or not in PATH"
  fi

  if ! docker ps &> /dev/null; then
    error "Docker daemon is not running. Start Docker and try again."
  fi
}

# Show usage
usage() {
  cat << 'EOF'
FraiseQL Test Database Manager

Usage: ./tests/docker/manage.sh [COMMAND] [OPTIONS]

Commands:

  start               Start all test databases (PostgreSQL, MySQL, SQL Server)
  stop                Stop all test databases
  restart             Restart all test databases
  status              Show database container status
  logs [SERVICE]      View logs (postgres|mysql|sqlserver|all)

  health              Check database connectivity
  clean               Stop and remove all containers and volumes

  test                Run integration tests (all databases)
  test-postgres       Run PostgreSQL adapter tests only
  test-mysql          Run MySQL adapter tests only
  test-sqlserver      Run SQL Server adapter tests only

  db-postgres         Connect to PostgreSQL directly
  db-mysql            Connect to MySQL directly
  db-sqlserver        Connect to SQL Server directly

  help                Show this help message

Examples:

  # Start databases and run tests
  ./tests/docker/manage.sh start
  ./tests/docker/manage.sh test

  # Check status
  ./tests/docker/manage.sh status
  ./tests/docker/manage.sh health

  # View logs
  ./tests/docker/manage.sh logs all
  ./tests/docker/manage.sh logs postgres

  # Run specific database tests
  ./tests/docker/manage.sh test-postgres

  # Connect directly
  ./tests/docker/manage.sh db-postgres

  # Cleanup
  ./tests/docker/manage.sh clean
EOF
}

# Start databases
start_databases() {
  check_docker
  info "Starting FraiseQL test databases..."

  cd "$PROJECT_ROOT"
  docker compose up -d

  info "Waiting for databases to be healthy..."

  local retries=30
  local count=0

  while [ $count -lt $retries ]; do
    if docker compose exec -T postgres pg_isready -U test_user -d fraiseql &> /dev/null && \
       docker compose exec -T mysql mysqladmin ping -u test_user -ptest_password &> /dev/null && \
       docker compose exec -T sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123 -Q "SELECT 1" -C &> /dev/null; then
      success "All databases are healthy!"
      return 0
    fi

    echo -n "."
    sleep 2
    ((count++))
  done

  warn "Database startup still in progress. SQL Server can take 60+ seconds."
  info "Current status:"
  docker compose ps
  return 0
}

# Stop databases
stop_databases() {
  check_docker
  info "Stopping FraiseQL test databases..."

  cd "$PROJECT_ROOT"
  docker compose down

  success "Databases stopped"
}

# Restart databases
restart_databases() {
  stop_databases
  sleep 2
  start_databases
}

# Show status
show_status() {
  check_docker
  info "FraiseQL Database Status:"

  cd "$PROJECT_ROOT"
  docker compose ps

  echo ""
  info "Database URLs:"
  echo "  PostgreSQL: postgresql://test_user:test_password@localhost:5432/fraiseql"
  echo "  MySQL:      mysql://test_user:test_password@localhost:3306/fraiseql"
  echo "  SQL Server: mssql://sa:SqlServer@123@localhost:1433/fraiseql"
}

# Check health
check_health() {
  check_docker
  info "Checking database connectivity..."

  cd "$PROJECT_ROOT"

  local pg_ok=false
  local mysql_ok=false
  local sqlserver_ok=false

  if docker compose exec -T postgres pg_isready -U test_user -d fraiseql &> /dev/null; then
    success "PostgreSQL is accessible"
    pg_ok=true
  else
    error "PostgreSQL is not responding"
  fi

  if docker compose exec -T mysql mysqladmin ping -u test_user -ptest_password &> /dev/null; then
    success "MySQL is accessible"
    mysql_ok=true
  else
    error "MySQL is not responding"
  fi

  if docker compose exec -T sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123 -Q "SELECT 1" -C &> /dev/null; then
    success "SQL Server is accessible"
    sqlserver_ok=true
  else
    error "SQL Server is not responding"
  fi

  if [ "$pg_ok" = true ] && [ "$mysql_ok" = true ] && [ "$sqlserver_ok" = true ]; then
    success "All databases are healthy!"
    return 0
  else
    error "One or more databases are not healthy"
    return 1
  fi
}

# Show logs
show_logs() {
  check_docker

  local service="${1:-all}"

  cd "$PROJECT_ROOT"

  case "$service" in
    postgres)
      info "PostgreSQL logs (Ctrl+C to exit):"
      docker compose logs -f postgres
      ;;
    mysql)
      info "MySQL logs (Ctrl+C to exit):"
      docker compose logs -f mysql
      ;;
    sqlserver)
      info "SQL Server logs (Ctrl+C to exit):"
      docker compose logs -f sqlserver
      ;;
    all)
      info "All database logs (Ctrl+C to exit):"
      docker compose logs -f postgres mysql sqlserver
      ;;
    *)
      error "Unknown service: $service (use: postgres, mysql, sqlserver, all)"
      ;;
  esac
}

# Clean everything
clean_all() {
  check_docker

  warn "This will stop and remove all containers and volumes"
  read -p "Continue? (y/n) " -n 1 -r
  echo

  if [[ $REPLY =~ ^[Yy]$ ]]; then
    info "Removing all containers and volumes..."
    cd "$PROJECT_ROOT"
    docker compose down -v
    success "Cleanup complete"
  else
    info "Cleanup cancelled"
  fi
}

# Run tests
run_tests() {
  check_docker

  info "Verifying databases are healthy..."

  cd "$PROJECT_ROOT"
  if ! docker compose ps | grep -q "healthy"; then
    warn "Not all databases are healthy. Starting them..."
    start_databases
  fi

  info "Running integration tests (all databases)..."
  cargo test --all-features
}

# Run specific database tests
run_db_tests() {
  check_docker

  local db="$1"

  info "Verifying databases are healthy..."

  cd "$PROJECT_ROOT"
  if ! docker compose ps | grep -q "healthy"; then
    warn "Not all databases are healthy. Starting them..."
    start_databases
  fi

  case "$db" in
    postgres)
      info "Running PostgreSQL adapter tests..."
      cargo test -p fraiseql-core db::postgres:: --test-threads=2
      ;;
    mysql)
      info "Running MySQL adapter tests..."
      cargo test -p fraiseql-core db::mysql:: --test-threads=2
      ;;
    sqlserver)
      info "Running SQL Server adapter tests (single-threaded)..."
      cargo test -p fraiseql-core db::sqlserver:: --test-threads=1
      ;;
    *)
      error "Unknown database: $db (use: postgres, mysql, sqlserver)"
      ;;
  esac
}

# Connect to database
connect_db() {
  check_docker

  local db="$1"

  cd "$PROJECT_ROOT"

  case "$db" in
    postgres)
      info "Connecting to PostgreSQL..."
      docker compose exec postgres psql -U test_user -d fraiseql
      ;;
    mysql)
      info "Connecting to MySQL..."
      docker compose exec mysql mysql -u test_user -p test_password -D fraiseql
      ;;
    sqlserver)
      info "Connecting to SQL Server..."
      docker compose exec sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123
      ;;
    *)
      error "Unknown database: $db (use: postgres, mysql, sqlserver)"
      ;;
  esac
}

# Main command handler
main() {
  local cmd="${1:-help}"

  case "$cmd" in
    start)
      start_databases
      ;;
    stop)
      stop_databases
      ;;
    restart)
      restart_databases
      ;;
    status)
      show_status
      ;;
    logs)
      show_logs "$2"
      ;;
    health)
      check_health
      ;;
    clean)
      clean_all
      ;;
    test)
      run_tests
      ;;
    test-postgres)
      run_db_tests "postgres"
      ;;
    test-mysql)
      run_db_tests "mysql"
      ;;
    test-sqlserver)
      run_db_tests "sqlserver"
      ;;
    db-postgres)
      connect_db "postgres"
      ;;
    db-mysql)
      connect_db "mysql"
      ;;
    db-sqlserver)
      connect_db "sqlserver"
      ;;
    help)
      usage
      ;;
    *)
      error "Unknown command: $cmd"
      usage
      exit 1
      ;;
  esac
}

main "$@"
