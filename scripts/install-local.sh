#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "=== MQ Local User-space Installer ==="
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"

if [[ ! -f "${REPO_ROOT}/target/release/mq" ]]; then
    echo "Сборка релизного бинарника..."
    (cd "${REPO_ROOT}" && cargo build --release)
fi

echo "Установка бинарника в ${BIN_DIR}/mq..."
install -m 755 "${REPO_ROOT}/target/release/mq" "${BIN_DIR}/mq"

echo "Установка иконки в ${ICON_DIR}/mq.svg..."
install -m 644 "${REPO_ROOT}/packaging/mq.svg" "${ICON_DIR}/mq.svg"

echo "Установка desktop-ярлыка в ${APP_DIR}/mq.desktop..."
install -m 644 "${REPO_ROOT}/packaging/mq.desktop" "${APP_DIR}/mq.desktop"

# Update desktop and icon databases if tools exist
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo ""
echo "Установка MQ успешно завершена!"
echo "Бинарник: ${BIN_DIR}/mq"
echo "Убедитесь, что ${BIN_DIR} находится в вашей переменной PATH."
echo "Запуск из терминала: mq"
echo "Либо через меню приложений вашей среды (GNOME / KDE / Hyprland / Rofi / Wofi)."
