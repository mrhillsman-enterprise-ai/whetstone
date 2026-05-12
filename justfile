# Whetstone — run tasks via `just <recipe>`

default:
    @just --list

# Set up git hooks (run once after clone)
init:
    git config core.hooksPath .githooks

# Build debug binary
build:
    cargo build

# Build optimized release binary
build-release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt --check

# Build, test, and lint in one shot
check: build test lint

# Release verification gate
release-check: fmt-check test lint

# Run whetstone setup (uses cargo run)
setup *ARGS:
    cargo run -- setup {{ARGS}}

# Run whetstone uninstall
uninstall:
    cargo run -- uninstall

# Bump version, push release branch, and open PR: just release patch|minor|major
release *ARGS: release-check
    cargo run -- release {{ARGS}}

# Deprecated legacy path kept for compatibility
release-publish *ARGS:
    cargo run -- release-publish {{ARGS}}
