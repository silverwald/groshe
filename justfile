# Groshe Task Runner

default: check test

# Check all workspace targets
check:
    cargo check --workspace --all-targets

# Run all workspace unit and integration tests
test:
    cargo test --workspace

# Build all workspace binaries and libraries
build:
    cargo build --workspace

# Format code with rustfmt
fmt:
    cargo fmt --all

# Lint with clippy
lint:
    cargo clippy --workspace --all-targets

# Run the CLI binary
cli *ARGS:
    cargo run -p groshe-cli -- {{ARGS}}

# Test core domain crate only
test-core:
    cargo test -p groshe-core

# Build Android native libraries for 64-bit ARM and x86_64 emulator
android-build PROFILE="debug":
    cargo ndk -t arm64-v8a -t x86_64 build -p groshe-android {{ if PROFILE == "release" { "--release" } else { "" } }}
