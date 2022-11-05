#!/usr/bin/env bash

# Local check that we are likely to pass CI
# CI is quite slow so it's nice to be able to run locally

set -o errexit
set -o xtrace

cargo fmt --all -- --check

cargo check --manifest-path=font-types/Cargo.toml --no-default-features
cargo check --manifest-path=read-fonts/Cargo.toml --no-default-features

cargo clippy --all-features --all-targets -- -D warnings
cargo run --bin=codegen resources/codegen_plan.toml
cargo test
