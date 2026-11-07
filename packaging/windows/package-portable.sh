#!/usr/bin/env bash
# ==============================================================================
#  MQ Launcher — Portable Windows Archive Builder
#  Produces: dist/mq-windows-x86_64-portable.zip
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

DIST_DIR="${REPO_ROOT}/dist"
PACKAGE_NAME="mq-windows-x86_64-portable"
ARCHIVE_PATH="${DIST_DIR}/${PACKAGE_NAME}.zip"

echo "=== Building MQ Windows Portable Archive ==="

# Resolve binary path
EXE_CANDIDATE="${1:-}"
if [[ -z "${EXE_CANDIDATE}" ]]; then
    if [[ -f "${REPO_ROOT}/target/x86_64-pc-windows-msvc/release/mq.exe" ]]; then
        EXE_CANDIDATE="${REPO_ROOT}/target/x86_64-pc-windows-msvc/release/mq.exe"
    elif [[ -f "${REPO_ROOT}/target/x86_64-pc-windows-gnu/release/mq.exe" ]]; then
        EXE_CANDIDATE="${REPO_ROOT}/target/x86_64-pc-windows-gnu/release/mq.exe"
    elif [[ -f "${REPO_ROOT}/target/release/mq.exe" ]]; then
        EXE_CANDIDATE="${REPO_ROOT}/target/release/mq.exe"
    else
        echo "Error: mq.exe not found in target directories. Please build it first:"
        echo "  cargo build --release --target x86_64-pc-windows-msvc"
        echo "Or specify path as argument: $0 /path/to/mq.exe"
        exit 1
    fi
fi

echo "Using binary: ${EXE_CANDIDATE}"
mkdir -p "${DIST_DIR}"

STAGE_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "${STAGE_DIR}"
}
trap cleanup EXIT

TARGET_DIR="${STAGE_DIR}/${PACKAGE_NAME}"
mkdir -p "${TARGET_DIR}"

# Copy payload items
cp "${EXE_CANDIDATE}" "${TARGET_DIR}/mq.exe"
cp "${SCRIPT_DIR}/portable.flag" "${TARGET_DIR}/portable.flag"
cp "${SCRIPT_DIR}/mq.ico" "${TARGET_DIR}/mq.ico"
cp "${SCRIPT_DIR}/run.bat" "${TARGET_DIR}/run.bat"
cp "${SCRIPT_DIR}/README-portable.txt" "${TARGET_DIR}/README.txt"

# Build zip archive
rm -f "${ARCHIVE_PATH}"
(
    cd "${STAGE_DIR}"
    zip -r -9 "${ARCHIVE_PATH}" "${PACKAGE_NAME}"
)

echo "Portable package created successfully:"
ls -lh "${ARCHIVE_PATH}"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${ARCHIVE_PATH}"
fi
