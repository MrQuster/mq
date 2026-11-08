#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "=== Сборка автономного портативного пакета MQ для друга ==="

cd "${REPO_ROOT}"

if [[ ! -f "target/release/mq" ]]; then
    echo "Сборка релизного бинарника..."
    cargo build --release
fi

STAGE_DIR="$(mktemp -d)"
PACKAGE_NAME="mq-portable-linux-x86_64"
TARGET_DIR="${STAGE_DIR}/${PACKAGE_NAME}"
mkdir -p "${TARGET_DIR}" "${REPO_ROOT}/dist"

cp "target/release/mq" "${TARGET_DIR}/mq"
chmod +x "${TARGET_DIR}/mq"

cp "packaging/mq.svg" "${TARGET_DIR}/mq.svg"
cp "packaging/mq.desktop" "${TARGET_DIR}/mq.desktop"

cat << 'EOF' > "${TARGET_DIR}/install.sh"
#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"

install -m 755 "${DIR}/mq" "${BIN_DIR}/mq"
install -m 644 "${DIR}/mq.svg" "${ICON_DIR}/mq.svg"
install -m 644 "${DIR}/mq.desktop" "${APP_DIR}/mq.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi

echo "MQ Launcher успешно установлен!"
echo "Запуск: ~/.local/bin/mq (или просто mq, если ~/.local/bin в PATH)"
echo "Также лаунчер доступен в системном меню приложений!"
EOF
chmod +x "${TARGET_DIR}/install.sh"

cat << 'EOF' > "${TARGET_DIR}/README.txt"
================================================================================
  MQ — МНОГОПОТОЧНЫЙ НЕОФИЦИАЛЬНЫЙ ЛАУНЧЕР MINECRAFT ДЛЯ LINUX
================================================================================

Как запустить без установки:
  ./mq

Как установить в систему пользователя (в ~/.local/bin и в меню приложений):
  ./install.sh

Основные возможности:
  1. Офлайн-режим: во вкладке Аккаунты введите любой ник — работает сразу.
  2. Ely.by: поддержка официальных скинов и плащей Ely.by (автоматическая загрузка
     authlib-injector и проверка контрольной суммы SHA-256).
  3. Microsoft: авторизация через Device Code Flow (код в браузере).
  4. Каталог модов и сборок Modrinth и CurseForge прямо в лаунчере.
  5. Полная многопоточность: проверка хэшей, распаковка и скачивание загружают
     все ядра вашего процессора.

Требования:
  - Linux x86_64 (Arch, Ubuntu, Fedora, Debian и др.)
  - Java 17 или 21 (например: sudo pacman -S jdk21-openjdk)
================================================================================
EOF

ARCHIVE_PATH="${REPO_ROOT}/dist/${PACKAGE_NAME}.tar.gz"
echo "Создание архива ${ARCHIVE_PATH}..."
tar -czf "${ARCHIVE_PATH}" -C "${STAGE_DIR}" "${PACKAGE_NAME}"
rm -rf "${STAGE_DIR}"

echo ""
echo "Архив для друга успешно создан:"
ls -lh "${ARCHIVE_PATH}"
sha256sum "${ARCHIVE_PATH}"
