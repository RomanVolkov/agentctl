.PHONY: all build test lint check install clean

all: build

build:
	cargo build --workspace

test:
	cargo test --workspace

lint:
	cargo clippy --all-targets -- -D warnings

check: test lint

install:
	cargo install --path agentctl --locked

clean:
	cargo clean
