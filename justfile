set shell := ["bash", "-euo", "pipefail", "-c"]

# What to run before committing
check: fmt-check lint test

# Format the Rust sources
fmt:
    cargo fmt

# Check formatting
fmt-check:
    cargo fmt --check

# Lint
lint:
    cargo clippy --all-targets -- -D warnings

# Unit and integration tests (offline)
test:
    cargo test

# Build the Nix package, tests included
build:
    nix build

# Run once: just run <config> <state-dir>
run config state:
    CONFIG_FILE={{config}} STATE_DIR={{state}} cargo run
