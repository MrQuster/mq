#!/usr/bin/env bash
# ==============================================================================
#  MQ Launcher — Automated Installer Tests
#  Runs isolated test cases against a local mock HTTP release server.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
INSTALL_SCRIPT="${REPO_ROOT}/install.sh"

echo "================================================================================"
echo " [TEST SUITE] MQ Linux Installer Automated Tests"
echo "================================================================================"

# Temporary directories for test run
TEST_WORKSPACE="$(mktemp -d -t mq-test-XXXXXX)"
MOCK_SERVER_DIR="${TEST_WORKSPACE}/mock_server"
MOCK_RELEASES_DIR="${MOCK_SERVER_DIR}/releases/latest/download"
mkdir -p "${MOCK_RELEASES_DIR}"

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
        kill "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
    rm -rf "${TEST_WORKSPACE}"
}
trap cleanup EXIT INT TERM

# Find available port
PORT=28951
for p in $(seq 28951 28999); do
    if ! ss -tuln 2>/dev/null | grep -q ":${p} "; then
        PORT="${p}"
        break
    fi
done

echo "-> Запуск локального mock-сервера на 127.0.0.1:${PORT}..."
(
    cd "${MOCK_SERVER_DIR}"
    python3 -m http.server "${PORT}" --bind 127.0.0.1 > "${TEST_WORKSPACE}/server.log" 2>&1
) &
SERVER_PID=$!
sleep 0.5

MOCK_BASE_URL="http://127.0.0.1:${PORT}/releases/latest/download"

# 1. Prepare mock release payload
create_mock_payload() {
    local payload_dir="${TEST_WORKSPACE}/payload"
    rm -rf "${payload_dir}"
    mkdir -p "${payload_dir}/mq-portable-linux-x86_64"

    # Create dummy mock executable
    cat << 'EOF' > "${payload_dir}/mq-portable-linux-x86_64/mq"
#!/bin/sh
echo "MQ Launcher Mock Executable v2.2.0"
exit 0
EOF
    chmod +x "${payload_dir}/mq-portable-linux-x86_64/mq"

    # Create dummy svg and desktop
    echo "<svg xmlns='http://www.w3.org/2000/svg'></svg>" > "${payload_dir}/mq-portable-linux-x86_64/mq.svg"
    cat << 'EOF' > "${payload_dir}/mq-portable-linux-x86_64/mq.desktop"
[Desktop Entry]
Name=MQ
Exec=mq %u
Icon=mq
Type=Application
EOF

    # Create tarball
    (
        cd "${payload_dir}"
        tar -czf "${MOCK_RELEASES_DIR}/mq-portable-linux-x86_64.tar.gz" mq-portable-linux-x86_64
    )

    # Copy standalone assets as well
    cp "${payload_dir}/mq-portable-linux-x86_64/mq" "${MOCK_RELEASES_DIR}/mq-linux-x86_64"
    cp "${payload_dir}/mq-portable-linux-x86_64/mq.svg" "${MOCK_RELEASES_DIR}/mq.svg"
    cp "${payload_dir}/mq-portable-linux-x86_64/mq.desktop" "${MOCK_RELEASES_DIR}/mq.desktop"

    # Generate SHA256SUMS
    (
        cd "${MOCK_RELEASES_DIR}"
        sha256sum mq-portable-linux-x86_64.tar.gz mq-linux-x86_64 mq.svg mq.desktop > SHA256SUMS
    )
}

create_mock_payload

# ------------------------------------------------------------------------------
# Test 1: Full Installation into isolated HOME
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 1: Стандартная установка в изолированный HOME..."
ISO_HOME="${TEST_WORKSPACE}/home1"
mkdir -p "${ISO_HOME}"

HOME="${ISO_HOME}" \
MQ_RELEASE_BASE_URL="${MOCK_BASE_URL}" \
sh "${INSTALL_SCRIPT}" --skip-deps

# Verify expected files exist
if [[ ! -x "${ISO_HOME}/.local/bin/mq" ]]; then
    echo "FAIL: ${ISO_HOME}/.local/bin/mq не найден или не исполняемый!" >&2
    exit 1
fi

if [[ ! -f "${ISO_HOME}/.local/share/applications/mq.desktop" ]]; then
    echo "FAIL: ${ISO_HOME}/.local/share/applications/mq.desktop не найден!" >&2
    exit 1
fi

if [[ ! -f "${ISO_HOME}/.local/share/icons/hicolor/scalable/apps/mq.svg" ]]; then
    echo "FAIL: ${ISO_HOME}/.local/share/icons/hicolor/scalable/apps/mq.svg не найден!" >&2
    exit 1
fi

# Verify desktop Exec path updated
if ! grep -q "${ISO_HOME}/.local/bin/mq" "${ISO_HOME}/.local/share/applications/mq.desktop"; then
    echo "FAIL: Desktop Exec не содержит путь к установленному бинарнику!" >&2
    exit 1
fi

echo "✓ TEST 1 PASSED: Все файлы установлены корректно."

# ------------------------------------------------------------------------------
# Test 2: Uninstallation and User Data Preservation
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 2: Деинсталляция и сохранение пользовательских данных..."
# Create mock user data
mkdir -p "${ISO_HOME}/.local/share/mq/instances/MyInstance"
echo '{"name": "test"}' > "${ISO_HOME}/.local/share/mq/instances/MyInstance/instance.json"
mkdir -p "${ISO_HOME}/.config/mq"
echo 'theme = "solid_matte"' > "${ISO_HOME}/.config/mq/config.toml"

HOME="${ISO_HOME}" \
sh "${INSTALL_SCRIPT}" --uninstall

# Check files removed
if [[ -f "${ISO_HOME}/.local/bin/mq" ]]; then
    echo "FAIL: ${ISO_HOME}/.local/bin/mq не был удален при --uninstall!" >&2
    exit 1
fi
if [[ -f "${ISO_HOME}/.local/share/applications/mq.desktop" ]]; then
    echo "FAIL: Desktop файл не был удален при --uninstall!" >&2
    exit 1
fi
if [[ -f "${ISO_HOME}/.local/share/icons/hicolor/scalable/apps/mq.svg" ]]; then
    echo "FAIL: Иконка не была удалена при --uninstall!" >&2
    exit 1
fi

# Check user data preserved!
if [[ ! -f "${ISO_HOME}/.local/share/mq/instances/MyInstance/instance.json" ]]; then
    echo "FAIL: Пользовательские данные в ~/.local/share/mq были стерты!" >&2
    exit 1
fi
if [[ ! -f "${ISO_HOME}/.config/mq/config.toml" ]]; then
    echo "FAIL: Конфигурация в ~/.config/mq была стерта!" >&2
    exit 1
fi

echo "✓ TEST 2 PASSED: Лаунчер удален, пользовательские данные сохранены."

# ------------------------------------------------------------------------------
# Test 3: SHA-256 verification failure on corrupted payload
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 3: Проверка защиты от повреждения / подмены файла (SHA-256)..."
ISO_HOME3="${TEST_WORKSPACE}/home3"
mkdir -p "${ISO_HOME3}"

# Corrupt the tarball without updating SHA256SUMS
echo "CORRUPTED CONTENT" >> "${MOCK_RELEASES_DIR}/mq-portable-linux-x86_64.tar.gz"

set +e
HOME="${ISO_HOME3}" \
MQ_RELEASE_BASE_URL="${MOCK_BASE_URL}" \
sh "${INSTALL_SCRIPT}" --skip-deps > "${TEST_WORKSPACE}/corrupt.log" 2>&1
EXIT_CODE=$?
set -e

if [[ "${EXIT_CODE}" -eq 0 ]]; then
    echo "FAIL: install.sh не выбросил ошибку при несовпадении SHA-256!" >&2
    exit 1
fi

if [[ -f "${ISO_HOME3}/.local/bin/mq" ]]; then
    echo "FAIL: Поврежденный бинарник был установлен в систему!" >&2
    exit 1
fi

echo "✓ TEST 3 PASSED: Поддельный/поврежденный файл отклонен с кодом ошибки."

# Restore clean payload
create_mock_payload

# ------------------------------------------------------------------------------
# Test 4: --dry-run Mode
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 4: Проверка режима --dry-run..."
ISO_HOME4="${TEST_WORKSPACE}/home4"
mkdir -p "${ISO_HOME4}"

HOME="${ISO_HOME4}" \
MQ_RELEASE_BASE_URL="${MOCK_BASE_URL}" \
sh "${INSTALL_SCRIPT}" --dry-run --skip-deps

if [[ -d "${ISO_HOME4}/.local" ]]; then
    echo "FAIL: В режиме --dry-run были созданы файлы или директории в HOME!" >&2
    exit 1
fi

echo "✓ TEST 4 PASSED: --dry-run не создал файлов в файловой системе."

# ------------------------------------------------------------------------------
# Test 5: Custom MQ_INSTALL_DIR / --install-dir
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 5: Установка в кастомную директорию через --install-dir..."
ISO_HOME5="${TEST_WORKSPACE}/home5"
CUSTOM_DIR="${TEST_WORKSPACE}/custom_bin"
mkdir -p "${ISO_HOME5}" "${CUSTOM_DIR}"

HOME="${ISO_HOME5}" \
MQ_RELEASE_BASE_URL="${MOCK_BASE_URL}" \
sh "${INSTALL_SCRIPT}" --install-dir "${CUSTOM_DIR}" --skip-deps

if [[ ! -x "${CUSTOM_DIR}/mq" ]]; then
    echo "FAIL: Бинарник не установлен в указанную через --install-dir директорию!" >&2
    exit 1
fi

echo "✓ TEST 5 PASSED: Кастомная директория установки поддержана."

# ------------------------------------------------------------------------------
# Test 6: Partial / Interrupted Download Safety Check
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 6: Защита от частичной загрузки скрипта (main() на последней строке)..."
ISO_HOME6="${TEST_WORKSPACE}/home6"
mkdir -p "${ISO_HOME6}"

# Simulate piping an incomplete truncated script into sh
set +e
HOME="${ISO_HOME6}" head -n 60 "${INSTALL_SCRIPT}" | sh >/dev/null 2>&1
set -e

# Crucial guarantee: no partial installation occurred
if [[ -f "${ISO_HOME6}/.local/bin/mq" ]]; then
    echo "FAIL: Частично загруженный скрипт произвел установку файлов!" >&2
    exit 1
fi

# Ensure the last line calls main
LAST_LINE="$(tail -n 1 "${INSTALL_SCRIPT}")"
if [[ "${LAST_LINE}" != 'main "$@"' ]]; then
    echo "FAIL: Последняя строка install.sh должна быть строго 'main \"\$@\"', обнаружено: ${LAST_LINE}" >&2
    exit 1
fi

echo "✓ TEST 6 PASSED: Все вызовы инкапсулированы внутри main() на финальной строке."

# ------------------------------------------------------------------------------
# Test 7: Piped invocation via stdin (`curl ... | sh -s -- [args]`)
# ------------------------------------------------------------------------------
echo ""
echo "-> TEST 7: Запуск через pipe (curl | sh -s -- [flags])..."
ISO_HOME7="${TEST_WORKSPACE}/home7"
mkdir -p "${ISO_HOME7}"

HOME="${ISO_HOME7}" \
MQ_RELEASE_BASE_URL="${MOCK_BASE_URL}" \
cat "${INSTALL_SCRIPT}" | sh -s -- --dry-run --skip-deps

echo "✓ TEST 7 PASSED: Запуск через pipe работает без сбоев."

echo ""
echo "================================================================================"
echo " [TEST SUITE] ВСЕ ТЕСТЫ УСТАНОВЩИКА УСПЕШНО ПРОЙДЕНЫ!"
echo "================================================================================"
