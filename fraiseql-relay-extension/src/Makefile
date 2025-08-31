# FraiseQL Relay PostgreSQL Extension Makefile
#
# This makefile builds the C extension for high-performance Relay operations

MODULE_big = fraiseql_relay
OBJS = fraiseql_relay.o

# Extension files
EXTENSION = fraiseql_relay
DATA = fraiseql_relay--1.0.sql
DOCS = README.md

# PostgreSQL build system integration
PG_CONFIG = pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)

# Compiler flags for optimization and debugging
PG_CPPFLAGS = -I$(shell $(PG_CONFIG) --includedir-server)
CFLAGS = -Wall -Wmissing-prototypes -Wpointer-arith -Wdeclaration-after-statement \
         -Wendif-labels -Wmissing-format-attribute -Wformat-security \
         -fno-strict-aliasing -fwrapv -fexcess-precision=standard \
         -O2 -g

# For debugging builds, uncomment:
# CFLAGS += -DDEBUG -O0

# Platform-specific settings
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    # macOS specific settings
    CFLAGS += -Wno-unused-command-line-argument
endif

ifeq ($(UNAME_S),Linux)
    # Linux specific settings
    CFLAGS += -fPIC
endif

# Include PostgreSQL build system
include $(PGXS)

# Custom targets
.PHONY: clean-all test install-dev benchmark

# Clean everything including generated files
clean-all: clean
	rm -f *.so *.o *.bc
	rm -f sql/*.backup

# Install for development (with debug symbols)
install-dev: CFLAGS += -DDEBUG -O0 -g3
install-dev: all install

# Run basic tests (requires test database)
test: install
	@echo "Running basic extension tests..."
	@psql -d test -c "CREATE EXTENSION IF NOT EXISTS fraiseql_relay;" || \
		(echo "Test database 'test' not found. Create with: createdb test"; exit 1)
	@psql -d test -c "SELECT core.fraiseql_relay_health();"
	@echo "Basic tests passed!"

# Performance benchmarks (requires test data)
benchmark: install test
	@echo "Running performance benchmarks..."
	@psql -d test -f tests/performance/benchmark.sql
	@echo "Benchmark complete!"

# Development helpers
format:
	@command -v clang-format >/dev/null 2>&1 && \
		clang-format -i src/*.c src/*.h || \
		echo "clang-format not found, skipping format"

lint:
	@command -v cppcheck >/dev/null 2>&1 && \
		cppcheck --enable=all --suppress=missingIncludeSystem src/ || \
		echo "cppcheck not found, skipping lint"

# Documentation generation
docs:
	@command -v doxygen >/dev/null 2>&1 && \
		doxygen Doxyfile || \
		echo "doxygen not found, skipping docs generation"

# Show build configuration
config:
	@echo "PostgreSQL Config: $(PG_CONFIG)"
	@echo "PGXS: $(PGXS)"
	@echo "Include Dir: $(shell $(PG_CONFIG) --includedir-server)"
	@echo "Library Dir: $(shell $(PG_CONFIG) --libdir)"
	@echo "Extension Dir: $(shell $(PG_CONFIG) --sharedir)/extension"
	@echo "Compiler Flags: $(CFLAGS)"

# Help target
help:
	@echo "FraiseQL Relay Extension Build Targets:"
	@echo ""
	@echo "  all          - Build the extension"
	@echo "  install      - Install the extension"
	@echo "  install-dev  - Install with debug symbols"
	@echo "  clean        - Clean build artifacts"
	@echo "  clean-all    - Clean everything"
	@echo "  test         - Run basic tests"
	@echo "  benchmark    - Run performance benchmarks"
	@echo "  format       - Format source code"
	@echo "  lint         - Run static analysis"
	@echo "  docs         - Generate documentation"
	@echo "  config       - Show build configuration"
	@echo "  help         - Show this help"
	@echo ""
	@echo "Prerequisites:"
	@echo "  - PostgreSQL development headers (postgresql-server-dev-*)"
	@echo "  - C compiler (gcc/clang)"
	@echo "  - make"
	@echo ""
	@echo "Example usage:"
	@echo "  make clean && make && sudo make install"
