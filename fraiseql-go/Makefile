.PHONY: build test test-verbose lint fmt fmt-check clean help coverage

help:
	@echo "Available commands:"
	@echo "  make build         - Build the module"
	@echo "  make test          - Run all tests"
	@echo "  make test-verbose  - Run tests with verbose output"
	@echo "  make coverage      - Run tests with coverage report"
	@echo "  make lint          - Run Go linter (if available)"
	@echo "  make fmt           - Format code"
	@echo "  make fmt-check     - Check code formatting"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make examples      - Run examples"

build:
	go build ./...

test:
	go test ./...

test-verbose:
	go test -v ./...

coverage:
	go test -coverprofile=coverage.out ./...
	go tool cover -html=coverage.out -o coverage.html
	@echo "Coverage report generated: coverage.html"

lint:
	@command -v golangci-lint >/dev/null 2>&1 || { echo "golangci-lint not installed"; exit 1; }
	golangci-lint run ./...

fmt:
	go fmt ./...

fmt-check:
	@if [ -n "$$(go fmt ./...)" ]; then echo "Code formatting issues found"; exit 1; fi

vet:
	go vet ./...

clean:
	rm -f coverage.out coverage.html schema.json
	go clean

examples:
	@echo "Running basic_schema example..."
	cd examples && go run basic_schema.go
	@echo "\nRunning analytics_schema example..."
	cd examples && go run analytics_schema.go

.DEFAULT_GOAL := help
