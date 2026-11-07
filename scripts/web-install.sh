#!/usr/bin/env bash
# MQ Launcher — 1-line Web Installer for Linux
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/USERNAME/mq/main/scripts/web-install.sh | bash

set -euo pipefail

REPO_USER="${MQ_REPO_USER:-MrQuster}"
REPO_NAME="mq"
RELEASE_TAG="${MQ_VERSION:-latest}"

BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"

echo "================================================================================"
echo "                   MQ Launcher — Автономный установщик"
echo "================================================================================"

ARCH="$(uname -m)"
OS="$(uname -s)"

if [[ "${OS}" != "Linux" || "${ARCH}" != "x86_64" ]]; then
    echo "Ошибка: MQ Launcher собран для Linux x86_64. Обнаружено: ${OS} ${ARCH}."
    exit 1
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

cd "${TMP_DIR}"

TARBALL="mq-portable-linux-x86_64.tar.gz"
if [[ "${RELEASE_TAG}" == "latest" ]]; then
    DOWNLOAD_URL="https://github.com/${REPO_USER}/${REPO_NAME}/releases/latest/download/${TARBALL}"
else
    DOWNLOAD_URL="https://github.com/${REPO_USER}/${REPO_NAME}/releases/download/${RELEASE_TAG}/${TARBALL}"
fi
RAW_BASE="https://raw.githubusercontent.com/${REPO_USER}/${REPO_NAME}/main"

echo "Проверка наличия готового пакета на GitHub Releases..."
if curl -fsSL -I "${DOWNLOAD_URL}" >/dev/null 2>&1; then
    echo "Скачивание портативного пакета с GitHub..."
    curl -fsSL -o "${TARBALL}" "${DOWNLOAD_URL}"
    tar -xzf "${TARBALL}"
    INSTALL_DIR="${TMP_DIR}/mq-portable-linux-x86_64"
    install -m 755 "${INSTALL_DIR}/mq" "${BIN_DIR}/mq"
    install -m 644 "${INSTALL_DIR}/mq.svg" "${ICON_DIR}/mq.svg"
    install -m 644 "${INSTALL_DIR}/mq.desktop" "${APP_DIR}/mq.desktop"
    sed -i -e "s|^Exec=.*|Exec=${BIN_DIR}/mq %u|" -e "s|^TryExec=.*|TryExec=${BIN_DIR}/mq|" "${APP_DIR}/mq.desktop"
else
    echo "Свежий релизный архив не найден по ссылке релизов."
    echo "Скачивание и установка из репозитория..."
    if command -v cargo >/dev/null 2>&1; then
        echo "Сборка из исходников через cargo..."
        cargo install --git "https://github.com/${REPO_USER}/${REPO_NAME}.git" mq-cli --root "${HOME}/.local"
        # Download assets
        curl -fsSL "${RAW_BASE}/packaging/mq.svg" -o "${ICON_DIR}/mq.svg"
        curl -fsSL "${RAW_BASE}/packaging/mq.desktop" -o "${APP_DIR}/mq.desktop"
        sed -i -e "s|^Exec=.*|Exec=${BIN_DIR}/mq %u|" -e "s|^TryExec=.*|TryExec=${BIN_DIR}/mq|" "${APP_DIR}/mq.desktop"
    else
        echo "Ошибка: Cargo не найден, а архив в GitHub Releases ещё не загружен."
        echo "Загрузите архив dist/${TARBALL} во вкладку Releases на GitHub."
        exit 1
    fi
fi

# Ensure ~/.cargo/bin symlink if ~/.cargo/bin exists in PATH to prevent conflicts
CARGO_BIN="${HOME}/.cargo/bin"
if [[ -d "${CARGO_BIN}" && "${BIN_DIR}" != "${CARGO_BIN}" ]]; then
    ln -sf "${BIN_DIR}/mq" "${CARGO_BIN}/mq" 2>/dev/null || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo ""
echo "================================================================================"
echo " MQ Launcher успешно установлен!"
echo " Путь к бинарнику: ${BIN_DIR}/mq"
echo " Запуск: mq (или ~/.local/bin/mq)"
echo " Ярлык добавлен в системное меню приложений!"
echo "================================================================================"
