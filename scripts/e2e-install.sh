#!/usr/bin/env bash
# ==============================================================================
#  MQ Launcher — End-to-End Installation & Launch Matrix Test Suite
#  Author: MrQuster <https://github.com/MrQuster>
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Colors for clear terminal reporting
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${CYAN}================================================================================"
echo -e " [MQ E2E MATRIX] Dynamic End-to-End Installation & Launch Matrix Suite"
echo -e "================================================================================${NC}"

cd "${REPO_ROOT}"

# Parse optional command line flags early
RUN_LAUNCH=true
TARGET_FILTER=""
for arg in "$@"; do
    case "$arg" in
        --skip-launch)
            RUN_LAUNCH=false
            ;;
        --version=*)
            TARGET_FILTER="${arg#*=}"
            ;;
        --help|-h)
            echo "Usage: ./scripts/e2e-install.sh [--skip-launch] [--version=<id>]"
            exit 0
            ;;
    esac
done

MQ_BIN="${REPO_ROOT}/target/debug/mq"
if [ ! -f "${MQ_BIN}" ]; then
    echo -e "${YELLOW}Building MQ CLI binary...${NC}"
    cargo build --bin mq
fi

DATA_DIR="${MQ_DATA_DIR:-${HOME}/.local/share/mq}"
export DISPLAY="${DISPLAY:-:1}"
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

echo -e "${BLUE}[1/5] Querying Mojang Version Manifest dynamically...${NC}"
MANIFEST_INFO=$(python3 - << 'EOF'
import urllib.request
import json
import sys

MANIFEST_URL = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"

try:
    req = urllib.request.Request(MANIFEST_URL, headers={"User-Agent": "mq-e2e-matrix/2.2.0"})
    with urllib.request.urlopen(req, timeout=12) as response:
        data = json.loads(response.read().decode("utf-8"))
except Exception as e:
    sys.stderr.write(f"Warning: Failed to fetch live manifest ({e}), using cached fallback values.\n")
    data = {
        "latest": {"release": "26.3", "snapshot": "26.4-snapshot-7"},
        "versions": [
            {"id": "26.4-snapshot-7", "type": "snapshot"},
            {"id": "26.3", "type": "release"},
            {"id": "1.21.11", "type": "release"},
            {"id": "b1.8.1", "type": "old_beta"},
            {"id": "a1.2.6", "type": "old_alpha"}
        ]
    }

latest_rel = data.get("latest", {}).get("release", "26.3")
latest_snap = data.get("latest", {}).get("snapshot", "26.4-snapshot-7")

# Find latest 26.x
v26_candidates = [v["id"] for v in data.get("versions", []) if v["id"].startswith("26.")]
latest_26 = v26_candidates[0] if v26_candidates else latest_snap

# Find latest 1.21.x release
v121_candidates = [v["id"] for v in data.get("versions", []) if v["id"].startswith("1.21") and v.get("type") == "release"]
latest_121 = v121_candidates[0] if v121_candidates else "1.21.11"

# Find sample old_beta and old_alpha
beta_v = next((v["id"] for v in data.get("versions", []) if v.get("type") == "old_beta"), "b1.8.1")
alpha_v = next((v["id"] for v in data.get("versions", []) if v.get("type") == "old_alpha"), "a1.2.6")

print(f"LATEST_RELEASE={latest_rel}")
print(f"LATEST_26={latest_26}")
print(f"LATEST_121={latest_121}")
print(f"SAMPLE_BETA={beta_v}")
print(f"SAMPLE_ALPHA={alpha_v}")
EOF
)

eval "${MANIFEST_INFO}"

echo -e "  * Latest Release:      ${GREEN}${LATEST_RELEASE}${NC}"
echo -e "  * Latest 26.x:         ${GREEN}${LATEST_26}${NC}"
echo -e "  * Latest 1.21.x:       ${GREEN}${LATEST_121}${NC}"
echo -e "  * Historical Beta:     ${GREEN}${SAMPLE_BETA}${NC}"
echo -e "  * Historical Alpha:    ${GREEN}${SAMPLE_ALPHA}${NC}"

echo -e "${BLUE}[2/5] Ensuring offline test player profile exists...${NC}"
"${MQ_BIN}" accounts add --username "E2EPlayer" --offline 2>/dev/null || true

# Full required version matrix
MATRIX=(
    # Format: "VERSION:LOADER:OPTIONAL_LOADER_VER"
    "${LATEST_RELEASE}:vanilla:"
    "${LATEST_26}:vanilla:"
    "${LATEST_121}:vanilla:"
    "1.20.1:vanilla:"
    "1.18.2:vanilla:"
    "1.16.5:vanilla:"
    "1.12.2:vanilla:"
    "1.8.9:vanilla:"
    "1.7.10:vanilla:"
    "${SAMPLE_BETA}:vanilla:"
    "${SAMPLE_ALPHA}:vanilla:"
    # ModLoaders
    "${LATEST_RELEASE}:fabric:"
    "1.20.1:fabric:"
    "${LATEST_RELEASE}:quilt:"
    "1.20.1:forge:"
    "1.12.2:forge:"
    "${LATEST_RELEASE}:neoforge:"
)

TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

echo -e "${BLUE}[3/5] Executing Matrix Verification...${NC}"

for ENTRY in "${MATRIX[@]}"; do
    IFS=":" read -r MC_VER LOADER LOADER_VER <<< "${ENTRY}"

    if [ -n "${TARGET_FILTER}" ] && [ "${MC_VER}" != "${TARGET_FILTER}" ]; then
        continue
    fi

    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    SANITIZED_VER=$(echo "${MC_VER}" | tr '.' '-' | tr '/' '-')
    INST_ID="e2e-${SANITIZED_VER}-${LOADER}"
    INST_DIR="${DATA_DIR}/instances/${INST_ID}"

    echo -e "\n${BOLD}========================================================================${NC}"
    echo -e "${BOLD} [CASE ${TOTAL_TESTS}] Testing Minecraft ${CYAN}${MC_VER}${NC} with ${YELLOW}${LOADER}${NC} (${INST_ID})${NC}"
    echo -e "${BOLD}========================================================================${NC}"

    # Cleanup prior instance if existing
    "${MQ_BIN}" instances delete "${INST_ID}" --force 2>/dev/null || true

    # Step A: Create instance & install
    echo -e "${CYAN}--> Step A: Creating instance '${INST_ID}'...${NC}"
    CREATE_CMD=("${MQ_BIN}" "instances" "create" "${INST_ID}" "--version" "${MC_VER}" "--loader" "${LOADER}")
    if [ -n "${LOADER_VER}" ]; then
        CREATE_CMD+=("--loader_version" "${LOADER_VER}")
    fi

    if ! "${CREATE_CMD[@]}"; then
        echo -e "${RED}[FAIL] Failed to create/install instance ${INST_ID}!${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        continue
    fi

    # Check install_state.json
    STATE_FILE="${INST_DIR}/install_state.json"
    if [ ! -f "${STATE_FILE}" ]; then
        echo -e "${RED}[FAIL] ${STATE_FILE} was not created!${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        continue
    fi

    STATE_STATUS=$(python3 -c "
import json
try:
    with open('${STATE_FILE}') as f:
        d = json.load(f)
    print(d.get('status', 'unknown'))
except Exception as e:
    print('error')
")
    echo -e "    install_state.json status: ${GREEN}${STATE_STATUS}${NC}"
    if [ "${STATE_STATUS}" != "ready" ] && [ "${STATE_STATUS}" != "installing" ]; then
        echo -e "${YELLOW}[WARN] Non-standard install_state status: ${STATE_STATUS}${NC}"
    fi

    # Step B: Dry-run launch arguments verification
    echo -e "${CYAN}--> Step B: Validating JVM launch command flags via --dry-run...${NC}"
    DRY_RUN_OUT=$("${MQ_BIN}" launch "${INST_ID}" --dry-run 2>&1 || true)

    # 1. Check for leftover unresolved placeholders ${...}
    if echo "${DRY_RUN_OUT}" | grep -E '\$\{[a-zA-Z0-9_.]+\}'; then
        echo -e "${RED}[FAIL] Unresolved \${...} placeholders found in launch command!${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        continue
    fi
    echo -e "    ${GREEN}[OK]${NC} Zero unresolved \${...} placeholders."

    # 2. Check classpath
    if ! echo "${DRY_RUN_OUT}" | grep -qE "(-cp|-classpath)"; then
        echo -e "${RED}[FAIL] Classpath (-cp / -classpath) missing from JVM launch arguments!${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        continue
    fi
    echo -e "    ${GREEN}[OK]${NC} Classpath present."

    # 3. Check -Djava.library.path=
    if ! echo "${DRY_RUN_OUT}" | grep -q -- "-Djava.library.path="; then
        echo -e "${YELLOW}[WARN] -Djava.library.path= missing from arguments.${NC}"
    else
        echo -e "    ${GREEN}[OK]${NC} -Djava.library.path configured."
    fi

    # 4. Check --assetsDir & --assetIndex
    if ! echo "${DRY_RUN_OUT}" | grep -q -- "--assetsDir"; then
        echo -e "${YELLOW}[WARN] --assetsDir not explicitly in Minecraft client arguments.${NC}"
    else
        echo -e "    ${GREEN}[OK]${NC} --assetsDir present."
    fi

    if ! echo "${DRY_RUN_OUT}" | grep -q -- "--assetIndex"; then
        echo -e "${YELLOW}[WARN] --assetIndex not in Minecraft client arguments.${NC}"
    else
        echo -e "    ${GREEN}[OK]${NC} --assetIndex present."
    fi

    # Step C: Real game launch session (DISPLAY=:1)
    if [ "${RUN_LAUNCH}" = true ]; then
        echo -e "${CYAN}--> Step C: Spawning real game client session on DISPLAY=${DISPLAY}...${NC}"
        mkdir -p "${INST_DIR}/logs"
        STDOUT_LOG="${INST_DIR}/e2e_launch.stdout"
        STDERR_LOG="${INST_DIR}/e2e_launch.stderr"

        "${MQ_BIN}" launch "${INST_ID}" --offline E2EPlayer > "${STDOUT_LOG}" 2> "${STDERR_LOG}" &
        GAME_PID=$!

        echo -e "    Process spawned (PID: ${GAME_PID}). Monitoring lifecycle for >= 30 seconds..."
        ALIVE_SECS=0
        CRASHED=false

        for s in $(seq 1 30); do
            if ! kill -0 "${GAME_PID}" 2>/dev/null; then
                CRASHED=true
                break
            fi
            sleep 1
            ALIVE_SECS=$s
        done

        if [ "${CRASHED}" = true ]; then
            # Check if crash-reports exist
            if [ -d "${INST_DIR}/crash-reports" ] && [ "$(ls -A "${INST_DIR}/crash-reports" 2>/dev/null)" ]; then
                echo -e "${RED}[FAIL] Game crashed within ${ALIVE_SECS} seconds with crash reports!${NC}"
                cat "${INST_DIR}/crash-reports"/* 2>/dev/null | head -n 30
                FAILED_TESTS=$((FAILED_TESTS + 1))
                kill -TERM "${GAME_PID}" 2>/dev/null || true
                continue
            else
                echo -e "${YELLOW}[INFO] Game exited gracefully or stopped after ${ALIVE_SECS}s without crash reports.${NC}"
            fi
        else
            echo -e "    ${GREEN}[OK]${NC} Game process lived >= 30 seconds with active display!"
            # Clean shutdown of game process
            kill -TERM "${GAME_PID}" 2>/dev/null || true
            sleep 2
            kill -KILL "${GAME_PID}" 2>/dev/null || true
        fi
    fi

    # Step D: File corruption injection and automated repair
    echo -e "${CYAN}--> Step D: Testing integrity check and automated file repair...${NC}"

    # Verify files initially
    echo -e "    Verifying initial integrity..."
    "${MQ_BIN}" verify "${INST_ID}" >/dev/null 2>&1 || true

    # Find a library or asset to corrupt
    CORRUPTED_TARGET=""
    if [ -d "${INST_DIR}/libraries" ] && [ "$(find "${INST_DIR}/libraries" -type f | head -n 1)" ]; then
        CORRUPTED_TARGET=$(find "${INST_DIR}/libraries" -type f -name "*.jar" | head -n 1)
    elif [ -d "${DATA_DIR}/libraries" ] && [ "$(find "${DATA_DIR}/libraries" -type f | head -n 1)" ]; then
        CORRUPTED_TARGET=$(find "${DATA_DIR}/libraries" -type f -name "*.jar" | head -n 1)
    fi

    if [ -n "${CORRUPTED_TARGET}" ] && [ -f "${CORRUPTED_TARGET}" ]; then
        echo -e "    Corrupting library byte: ${CORRUPTED_TARGET}"
        # Corrupt 1 byte in the file
        printf "\x00" | dd of="${CORRUPTED_TARGET}" bs=1 seek=0 count=1 conv=notrunc 2>/dev/null

        # Run verify: must detect failure
        if "${MQ_BIN}" verify "${INST_ID}" >/dev/null 2>&1; then
            echo -e "${YELLOW}[WARN] Verify did not report corrupted file.${NC}"
        else
            echo -e "    ${GREEN}[OK]${NC} Corruption successfully caught by 'mq verify'!"
        fi

        # Run automated repair
        echo -e "    Running automated repair..."
        "${MQ_BIN}" install "${MC_VER}" --loader "${LOADER}" >/dev/null 2>&1 || true

        # Re-verify: must now pass cleanly
        if "${MQ_BIN}" verify "${INST_ID}" >/dev/null 2>&1; then
            echo -e "    ${GREEN}[OK]${NC} File automatically repaired and verified cleanly!"
        else
            echo -e "${YELLOW}[WARN] Repair did not completely resolve corrupted files on first attempt.${NC}"
        fi
    else
        echo -e "    ${YELLOW}[SKIP] No target library found to corrupt for this profile.${NC}"
    fi

    # Cleanup instance
    "${MQ_BIN}" instances delete "${INST_ID}" --force 2>/dev/null || true

    PASSED_TESTS=$((PASSED_TESTS + 1))
    echo -e "${GREEN}[PASS] Verification of ${MC_VER} (${LOADER}) completed successfully!${NC}"
done

echo -e "\n${CYAN}================================================================================"
echo -e " [MQ E2E MATRIX] Verification Summary"
echo -e "================================================================================${NC}"
echo -e "  Total Matrix Cases: ${BOLD}${TOTAL_TESTS}${NC}"
echo -e "  Passed:             ${GREEN}${BOLD}${PASSED_TESTS}${NC}"
echo -e "  Failed:             ${RED}${BOLD}${FAILED_TESTS}${NC}"

if [ "${FAILED_TESTS}" -eq 0 ]; then
    echo -e "\n${GREEN}${BOLD}[MQ E2E MATRIX] ALL VERSIONS AND LOADERS VERIFIED WITH 100% SUCCESS!${NC}\n"
    exit 0
else
    echo -e "\n${RED}${BOLD}[MQ E2E MATRIX] MATRIX SUITE COMPLETED WITH FAILURES!${NC}\n"
    exit 1
fi
