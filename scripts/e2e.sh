#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_ROOT}"

echo "================================================================================"
echo " [MQ E2E VERIFICATION GATE] Starting Full System Verification Suite v2.0.0"
echo "================================================================================"

MQ_BIN="${REPO_ROOT}/target/debug/mq"
if [ ! -f "${MQ_BIN}" ]; then
    echo "Building MQ binary..."
    cargo build --bin mq
fi

echo "--- 1. Testing System Diagnostics (mq doctor) ---"
"${MQ_BIN}" doctor

echo "--- 2. Testing Accounts Management ---"
"${MQ_BIN}" accounts list
"${MQ_BIN}" accounts add --username E2ETestHero --offline
"${MQ_BIN}" accounts list | grep -q "E2ETestHero"
"${MQ_BIN}" accounts switch Mr_Quster || true
"${MQ_BIN}" accounts remove E2ETestHero
echo "Accounts verification passed."

echo "--- 3. Testing Java Management ---"
"${MQ_BIN}" java list
echo "Java verification passed."

echo "--- 4. Testing Multi-Instance Lifecycle (create, clone, export, import, delete) ---"
"${MQ_BIN}" instances delete e2e-test-inst 2>/dev/null || true
"${MQ_BIN}" instances delete e2e-test-cloned 2>/dev/null || true
"${MQ_BIN}" instances delete e2e-test-imported 2>/dev/null || true
"${MQ_BIN}" instances list
"${MQ_BIN}" instances create e2e-test-inst --version 1.20.1
"${MQ_BIN}" instances list | grep -q "e2e-test-inst"

"${MQ_BIN}" instances clone e2e-test-inst e2e-test-cloned
"${MQ_BIN}" instances list | grep -q "e2e-test-cloned"

TEST_EXPORT="/tmp/mq-e2e-export-$$.zip"
"${MQ_BIN}" instances export --out "${TEST_EXPORT}" e2e-test-cloned
test -f "${TEST_EXPORT}"
"${MQ_BIN}" instances delete e2e-test-cloned

"${MQ_BIN}" instances import "${TEST_EXPORT}" --name "e2e-test-imported"
"${MQ_BIN}" instances list | grep -q "e2e-test-imported"
"${MQ_BIN}" instances delete e2e-test-imported
rm -f "${TEST_EXPORT}"
echo "Instance lifecycle verification passed."

echo "--- 5. Testing World Saves (list, backup, restore) ---"
DATA_DIR="${HOME}/.local/share/mq"
WORLD_DIR="${DATA_DIR}/instances/e2e-test-inst/saves/E2EWorld"
mkdir -p "${WORLD_DIR}"
echo "world_level_mock" > "${WORLD_DIR}/level.dat"

"${MQ_BIN}" worlds list e2e-test-inst | grep -q "E2EWorld"
"${MQ_BIN}" worlds backup e2e-test-inst E2EWorld

BACKUP_FILE=$(ls -t "${DATA_DIR}/backups/e2e-test-inst"/E2EWorld_*.zip | head -n 1)
test -f "${BACKUP_FILE}"

rm -rf "${WORLD_DIR}"
"${MQ_BIN}" worlds restore e2e-test-inst "${BACKUP_FILE}"
test -f "${WORLD_DIR}/level.dat"
echo "World saves verification passed."

echo "--- 6. Testing Launch Pipeline & Critical Bug #1 (Languages & Arguments) ---"
DRY_RUN_OUT=$("${MQ_BIN}" launch 26.3-vanilla --dry-run 2>&1)

echo "${DRY_RUN_OUT}" | grep -q -- "--assetsDir ${HOME}/.local/share/mq/assets"
echo "${DRY_RUN_OUT}" | grep -q -- "--assetIndex 34"

# Verify options.txt contains lang:ru_ru
OPTIONS_FILE="${DATA_DIR}/instances/26.3-vanilla/options.txt"
test -f "${OPTIONS_FILE}"
grep -q "lang:ru_ru" "${OPTIONS_FILE}"
echo "Critical Bug #1 (Languages and launch flags) verification passed."

# Cleanup test instance
"${MQ_BIN}" instances delete e2e-test-inst

echo "--- 7. Testing Offscreen UI Snapshot Generation (Vulkan/WGPU Headless) ---"
mkdir -p docs/screenshots
for SCREEN in instances detail catalog accounts perf settings; do
    OUT_PNG="docs/screenshots/00_${SCREEN}.png"
    echo "Rendering snapshot for screen: ${SCREEN} -> ${OUT_PNG}"
    "${MQ_BIN}" ui-snapshot --screen "${SCREEN}" --out "${OUT_PNG}"
    test -f "${OUT_PNG}"
    FILE_SIZE=$(wc -c < "${OUT_PNG}")
    if [ "${FILE_SIZE}" -lt 20000 ]; then
        echo "Error: snapshot image ${OUT_PNG} too small (${FILE_SIZE} bytes)"
        exit 1
    fi
    rm -f "${OUT_PNG}"
done
echo "UI snapshots verification passed."

echo "--- 8. Testing Integration Mock Server Suite ---"
cargo test -p testsupport

echo "================================================================================"
echo " [MQ E2E VERIFICATION GATE] ALL CHECKS PASSED WITH 100% SUCCESS!"
echo "================================================================================"
