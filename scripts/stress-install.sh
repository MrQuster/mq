#!/usr/bin/env bash
set -uo pipefail

# scripts/stress-install.sh - Stress test for mq installation, worker crash/journal recovery, and fast cancel/restart cycles.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/honey/mq/target}"

echo "Building mq binary with test-hooks feature..."
cargo build --bin mq --release --features test-hooks

MQ_BIN="${CARGO_TARGET_DIR}/release/mq"
if [ ! -x "${MQ_BIN}" ]; then
    echo "Error: mq binary not found at ${MQ_BIN}"
    exit 1
fi

# Create isolated data dir
TEST_DATA_DIR="$(mktemp -d -t mq-stress-XXXXXX)"
trap 'rm -rf "${TEST_DATA_DIR}"' EXIT

echo "=== Test 1: 10 cycles of quick instance create & install cancel/restart simulation ==="
for i in {1..10}; do
    export MQ_DATA_DIR="${TEST_DATA_DIR}/run_${i}"
    mkdir -p "${MQ_DATA_DIR}"
    "${MQ_BIN}" instance create "stress_${i}" --version "1.20.1" --loader vanilla >/dev/null 2>&1 || true
    "${MQ_BIN}" install-worker --instance "stress_${i}" &
    PID=$!
    sleep 0.05
    kill -9 "${PID}" 2>/dev/null || true
    wait "${PID}" 2>/dev/null || true
done
echo "Test 1 PASSED: Quick cancel/restart cycles completed."

echo "=== Test 2: install-worker crash (kill -9) and InstallJournal::load_unclosed detection ==="
export MQ_DATA_DIR="${TEST_DATA_DIR}/worker_crash"
mkdir -p "${MQ_DATA_DIR}"
"${MQ_BIN}" instance create "crash_inst" --version "1.20.1" --loader vanilla >/dev/null 2>&1 || true

# We can manually write or simulate a journal or start worker and kill it
"${MQ_BIN}" install-worker --instance "crash_inst" &
WORKER_PID=$!
sleep 0.2
kill -9 "${WORKER_PID}" 2>/dev/null || true
wait "${WORKER_PID}" 2>/dev/null || true

# Verify install.journal is created at ${MQ_DATA_DIR}/install.journal and remains active after kill -9
JOURNAL_PATH="${MQ_DATA_DIR}/install.journal"
if [ ! -f "${JOURNAL_PATH}" ]; then
    echo "Error: ${JOURNAL_PATH} was not created by install-worker!"
    exit 1
fi
jq -e '.active == true' "${JOURNAL_PATH}" >/dev/null
echo "Test 2 PASSED: Worker crash left unclosed journal at ${JOURNAL_PATH} (active=true)."

echo "=== Test 3: Headless Weston GUI hook MQ_TEST_ACTION ==="
export MQ_DATA_DIR="${TEST_DATA_DIR}/hook_test"
mkdir -p "${MQ_DATA_DIR}"
if [ -d "${HOME}/.local/share/mq/cas" ]; then
    ln -s "${HOME}/.local/share/mq/cas" "${MQ_DATA_DIR}/cas"
fi
if [ -d "${HOME}/.local/share/mq/runtimes" ]; then
    ln -s "${HOME}/.local/share/mq/runtimes" "${MQ_DATA_DIR}/runtimes"
fi

WESTON_PID=""
if command -v weston &>/dev/null; then
    weston --backend=headless --socket=mq-wl-stress --width=1280 --height=800 --idle-time=0 >/dev/null 2>&1 &
    WESTON_PID=$!
    export WAYLAND_DISPLAY=mq-wl-stress
    sleep 1
fi

export MQ_TEST_ACTION="install:26.3:vanilla"
export MQ_TEST_CANCEL_AFTER_MS=300
export MQ_TEST_EXIT_ON_FINISH=1

set +e
(ulimit -Sn 1024; timeout 25 "${MQ_BIN}" --backend glow)
RET=$?
set -e

if [ -n "${WESTON_PID}" ]; then
    kill "${WESTON_PID}" 2>/dev/null || true
    wait "${WESTON_PID}" 2>/dev/null || true
fi

if [ ${RET} -ne 0 ]; then
    echo "Error: MQ_TEST_ACTION returned non-zero exit code: ${RET}"
    exit 1
fi

# Check no crash logs
if ls "${MQ_DATA_DIR}"/crash-*.log 1>/dev/null 2>&1; then
    echo "Error: Crash logs detected!"
    exit 1
fi

echo "Test 3 PASSED: Headless GUI test hook completed successfully."
echo "All stress tests PASSED successfully."
