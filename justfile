default:
    @just --list

run *args:
    @go run ./cmd/scoutly {{ args }}

build:
    @go build -o scoutly ./cmd/scoutly

fmt:
    @gofmt -l -w .
    @golangci-lint fmt

lint:
    @go vet ./...
    @golangci-lint run

tidy:
    @go mod tidy

test:
    @go test -race -count=1 ./...

test-cover:
    @go test -race -count=1 -covermode=atomic -coverprofile=coverage.out ./...
    @go tool cover -func=coverage.out

clean:
    @rm -rf scoutly coverage.out
