#!/usr/bin/env bash
# ==============================================================================
#  MQ Launcher — 1-Command Universal Linux Installer
#  Usage:
#    curl -fsSL https://raw.githubusercontent.com/MrQuster/mq/main/install.sh | bash
# ==============================================================================

set -euo pipefail

REPO_USER="${MQ_REPO_USER:-MrQuster}"
REPO_NAME="${MQ_REPO_NAME:-mq}"
TARBALL="mq-portable-linux-x86_64.tar.gz"
RELEASE_URL="https://github.com/${REPO_USER}/${REPO_NAME}/releases/latest/download/${TARBALL}"
RAW_BASE="https://raw.githubusercontent.com/${REPO_USER}/${REPO_NAME}/main"

BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_SCALABLE="${HOME}/.local/share/icons/hicolor/scalable/apps"
ICON_128="${HOME}/.local/share/icons/hicolor/128x128/apps"

# ANSI Colors
BOLD="\033[1m"
GREEN="\033[1;32m"
BLUE="\033[1;34m"
YELLOW="\033[1;33m"
CYAN="\033[1;36m"
RED="\033[1;31m"
RESET="\033[0m"

echo -e "${CYAN}================================================================================"
echo -e "         MQ Launcher — Solid Edition (1-Command Installer)"
echo -e "================================================================================${RESET}"

# 1. Architecture & OS Check
ARCH="$(uname -m)"
OS="$(uname -s)"
if [[ "${OS}" != "Linux" || "${ARCH}" != "x86_64" ]]; then
    echo -e "${RED}Ошибка: MQ Launcher поддерживает только Linux x86_64 (обнаружено: ${OS} ${ARCH}).${RESET}"
    exit 1
fi

# 2. Arch Linux package check & Java 21+ installation
if command -v pacman >/dev/null 2>&1; then
    if ! pacman -Qs jre21-openjdk >/dev/null 2>&1 && ! pacman -Qs jdk21-openjdk >/dev/null 2>&1 && ! command -v java >/dev/null 2>&1; then
        echo -e "${YELLOW}[Arch Linux] Проверка Java: рекомендуется jre21-openjdk для современных версий Minecraft.${RESET}"
        if command -v sudo >/dev/null 2>&1; then
            echo -e "${BLUE}Установка jre21-openjdk через pacman...${RESET}"
            sudo pacman -S --needed --noconfirm jre21-openjdk || true
        fi
    fi
fi

mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_SCALABLE}" "${ICON_128}"

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT
cd "${TMP_DIR}"

INSTALLED_SUCCESS=0

# 3. Direct Fast Download from GitHub Releases (3 seconds!)
echo -e "${BLUE}[1/4] Поиск скомпилированного релиза на GitHub Releases...${RESET}"
if curl -fsSL -I "${RELEASE_URL}" >/dev/null 2>&1; then
    echo -e "${GREEN}Скачивание готового портативного архива v1.1.0 (3 секунды)...${RESET}"
    if curl -fL --progress-bar -o "${TARBALL}" "${RELEASE_URL}"; then
        tar -xzf "${TARBALL}"
        PKG_DIR="${TMP_DIR}/mq-portable-linux-x86_64"
        if [[ -f "${PKG_DIR}/mq" ]]; then
            install -m 755 "${PKG_DIR}/mq" "${BIN_DIR}/mq"
            [[ -f "${PKG_DIR}/mq.svg" ]] && install -m 644 "${PKG_DIR}/mq.svg" "${ICON_SCALABLE}/mq.svg"
            [[ -f "${PKG_DIR}/mq.desktop" ]] && install -m 644 "${PKG_DIR}/mq.desktop" "${APP_DIR}/mq.desktop"
            INSTALLED_SUCCESS=1
        fi
    fi
fi

# 4. Fallback: Clone & Cargo build if precompiled release was not reached
if [[ "${INSTALLED_SUCCESS}" -eq 0 ]]; then
    echo -e "${YELLOW}Предсобранный релиз не найден или недоступен. Запуск автоматической сборки через Cargo...${RESET}"
    if command -v cargo >/dev/null 2>&1 && command -v git >/dev/null 2>&1; then
        echo -e "${BLUE}Клонирование репозитория https://github.com/${REPO_USER}/${REPO_NAME}.git...${RESET}"
        git clone --depth 1 "https://github.com/${REPO_USER}/${REPO_NAME}.git" "${TMP_DIR}/mq-source"
        cd "${TMP_DIR}/mq-source"
        cargo build --release
        install -m 755 "target/release/mq" "${BIN_DIR}/mq"
        install -m 644 "packaging/mq.svg" "${ICON_SCALABLE}/mq.svg"
        install -m 644 "packaging/mq.desktop" "${APP_DIR}/mq.desktop"
        INSTALLED_SUCCESS=1
    else
        echo -e "${RED}Ошибка: Cargo или Git не найдены. Установите Rust (rustup) или загрузите релиз в GitHub Releases.${RESET}"
        exit 1
    fi
fi

# 5. Fallback icons & desktop file if missing
if [[ ! -f "${ICON_SCALABLE}/mq.svg" ]]; then
    curl -fsSL "${RAW_BASE}/packaging/mq.svg" -o "${ICON_SCALABLE}/mq.svg" 2>/dev/null || true
fi
if [[ ! -f "${APP_DIR}/mq.desktop" ]]; then
    curl -fsSL "${RAW_BASE}/packaging/mq.desktop" -o "${APP_DIR}/mq.desktop" 2>/dev/null || true
fi

# 6. Ensure ~/.local/bin in PATH
SHELL_CONFIGS=("${HOME}/.bashrc" "${HOME}/.zshrc")
for sh_rc in "${SHELL_CONFIGS[@]}"; do
    if [[ -f "${sh_rc}" ]]; then
        if ! grep -q '\.local/bin' "${sh_rc}"; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "${sh_rc}"
        fi
    fi
done

# 7. Update Desktop Database & Icon Cache
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo -e "\n${GREEN}================================================================================"
echo -e " ${BOLD}ПОЗДРАВЛЯЕМ! MQ LAUNCHER УСПЕШНО УСТАНОВЛЕН!${RESET}${GREEN}"
echo -e "================================================================================${RESET}"
echo -e " ${CYAN}Исполняемый файл:${RESET}  ${BIN_DIR}/mq"
echo -e " ${CYAN}Команда запуска:${RESET}   ${BOLD}mq${RESET} (или ${HOME}/.local/bin/mq)"
echo -e " ${CYAN}Ярлык меню:${RESET}        ~/.local/share/applications/mq.desktop"
echo -e " ${CYAN}Особенности:${RESET}       Solid Matte UI, Оффлайн (Пиратка) в 1 клик, Ely.by, Modrinth & CurseForge"
echo -e "${GREEN}================================================================================${RESET}\n"
