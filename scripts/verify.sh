#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/honey/mq/target}"

TMP_DIR="$(mktemp -d -t mq-verify-XXXXXX)"
trap 'rm -rf "${TMP_DIR}"' EXIT

declare -a CHECK_NAMES
declare -a CHECK_STATUSES
declare -a CHECK_LOGS

run_check() {
    local name="$1"
    local cmd="$2"
    local index="$3"
    local log_file="${TMP_DIR}/check_${index}.log"

    CHECK_NAMES+=("${name}")
    CHECK_LOGS+=("${log_file}")

    if eval "${cmd}" > "${log_file}" 2>&1; then
        CHECK_STATUSES+=("PASS")
    else
        CHECK_STATUSES+=("FAIL")
    fi
}

echo "Running verification suite in ${REPO_ROOT}..."

# 1. Formatting check
run_check "cargo fmt --all -- --check" "cargo fmt --all -- --check" "1"

# 2. Clippy check across workspace
run_check "cargo clippy --workspace --all-targets --locked -- -D warnings" "cargo clippy --workspace --all-targets --locked -- -D warnings" "2"

# 3. Workspace test suite
run_check "cargo test --workspace --locked" "cargo test --workspace --locked" "3"

# 4. Forbidden stubs and placeholder markers
run_check "No todo!/unimplemented!/TODO/FIXME/Coming soon in crates/" "! rg -n '\b(todo!|unimplemented!)\(|\bTODO\b|\bFIXME\b|Coming soon' crates/" "4"

# 5. The real CurseForge key must never be committed. Read it from the local
#    0600 env file (or environment) so the secret itself is not stored here.
CF_KEY=""
if [ -f "${HOME}/.config/mq/dev.env" ]; then
    CF_KEY="$(bash -c 'set -a; . "$1" >/dev/null 2>&1; printf %s "${MQ_CURSEFORGE_API_KEY:-${MQ_CF_API_KEY:-}}"' _ "${HOME}/.config/mq/dev.env")"
fi
if [ -z "${CF_KEY}" ]; then
    CF_KEY="${MQ_CURSEFORGE_API_KEY:-${MQ_CF_API_KEY:-}}"
fi
if [ -z "${CF_KEY}" ]; then
    run_check "CurseForge key not committed (key unavailable, checked patterns only)" "! rg -n 'x-api-key[[:space:]]*[:=][[:space:]]*[A-Za-z0-9]{8,}' crates/ docs/ README* CHANGELOG.md" "5"
else
    CF_KEY_FILE="${TMP_DIR}/cf_key"
    (umask 077; printf '%s' "${CF_KEY}" > "${CF_KEY_FILE}")
    run_check "CurseForge key not committed (working tree)" "! rg -nF -f '${CF_KEY_FILE}' crates/ docs/ README* CHANGELOG.md testsupport/ scripts/" "5"
fi

# Print Table
echo ""
echo "================================================================================"
echo "                           MQ VERIFICATION SUMMARY                              "
echo "================================================================================"
printf "%-68s | %-8s\n" "Check" "Result"
echo "---------------------------------------------------------------------+----------"

OVERALL_EXIT=0
for i in "${!CHECK_NAMES[@]}"; do
    printf "%-68s | %-8s\n" "${CHECK_NAMES[$i]}" "${CHECK_STATUSES[$i]}"
    if [ "${CHECK_STATUSES[$i]}" != "PASS" ]; then
        OVERALL_EXIT=1
    fi
done
echo "================================================================================"

# If failed, print last 30 lines for each failing check
if [ ${OVERALL_EXIT} -ne 0 ]; then
    echo ""
    echo "================================================================================"
    echo "                              FAILURE DETAILS                                   "
    echo "================================================================================"
    for i in "${!CHECK_NAMES[@]}"; do
        if [ "${CHECK_STATUSES[$i]}" != "PASS" ]; then
            echo ""
            echo "--- [FAIL] ${CHECK_NAMES[$i]} (last 30 lines) ---"
            tail -n 30 "${CHECK_LOGS[$i]}"
        fi
    done
    echo "================================================================================"
    exit 1
fi

echo "All checks passed successfully."
exit 0
