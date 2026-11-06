#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

echo "================================================================================"
echo " [MQ QA GATE] 1. Formatting check (cargo fmt --check)"
echo "================================================================================"
cargo fmt --all --check

echo "================================================================================"
echo " [MQ QA GATE] 2. Clippy linter across all targets (cargo clippy -D warnings)"
echo "================================================================================"
cargo clippy --workspace --all-targets -- -D warnings

echo "================================================================================"
echo " [MQ QA GATE] 3. Unit, integration, and property tests (cargo test)"
echo "================================================================================"
cargo test --workspace

echo "================================================================================"
echo " [MQ QA GATE] ALL CHECKS PASSED SUCCESSFULLY!"
echo "================================================================================"
