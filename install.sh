#!/bin/sh
# ==============================================================================
#  MQ Launcher — 1-Command Universal Linux Installer
#
#  Usage:
#    curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
#
#  Advanced Usage:
#    curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh -s -- [FLAGS]
#    sh install.sh [--version <tag>] [--uninstall] [--dry-run] [--install-dir <dir>]
# ==============================================================================

set -eu
(set -o pipefail 2>/dev/null) && set -o pipefail

# ANSI color formatting (enabled only when writing to an interactive terminal)
init_colors() {
    if [ -t 1 ] && { [ "${CLICOLOR_FORCE:-0}" != "0" ] || [ -z "${NO_COLOR:-}" ]; }; then
        BOLD="\033[1m"
        GREEN="\033[1;32m"
        BLUE="\033[1;34m"
        YELLOW="\033[1;33m"
        CYAN="\033[1;36m"
        RED="\033[1;31m"
        RESET="\033[0m"
    else
        BOLD=""
        GREEN=""
        BLUE=""
        YELLOW=""
        CYAN=""
        RED=""
        RESET=""
    fi
}

log_info() {
    printf "${CYAN}[MQ]${RESET} %s\n" "$1"
}

log_step() {
    printf "${BLUE}==>${RESET} ${BOLD}%s${RESET}\n" "$1"
}

log_success() {
    printf "${GREEN}[✓]${RESET} %s\n" "$1"
}

log_warn() {
    printf "${YELLOW}[!] Предупреждение:${RESET} %s\n" "$1" >&2
}

log_error() {
    printf "${RED}[✗] Ошибка:${RESET} %s\n" "$1" >&2
}

show_help() {
    cat << 'EOF'
MQ Launcher — Solid Edition (1-Command Linux Installer)

Использование:
  curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
  sh install.sh [ОПЦИИ]

Опции:
  -v, --version <TAG>       Установить конкретную версию/тег (по умолчанию: latest)
  -u, --uninstall           Удалить лаунчер (бинарник, ярлык, иконку). Пользовательские данные сохраняются!
  -n, --dry-run             Симуляция выполнения без изменения файлов в системе
      --install-dir <DIR>   Директория для установки бинарника (или через переменную MQ_INSTALL_DIR)
      --skip-deps           Пропустить проверку зависимостей динамических библиотек (ldd)
      --minisign-key <PUB>  Публичный ключ Minisign для проверки подписи SHA256SUMS
  -h, --help                Показать эту справку и выйти

Переменные окружения:
  MQ_VERSION                Версия для установки (эквивалент --version)
  MQ_INSTALL_DIR            Путь установки бинарника (по умолчанию: ~/.local/bin)
  MQ_MINISIGN_PUBKEY        Публичный ключ Minisign
  MQ_RELEASE_BASE_URL       Пользовательский URL для загрузки релизов (для тестов / зеркал)
  MQ_REPO_USER              Владелец репозитория GitHub (по умолчанию: MrQuster)
  MQ_REPO_NAME              Имя репозитория (по умолчанию: mq)
EOF
}

check_os_arch() {
    os_name="$(uname -s)"
    arch_name="$(uname -m)"

    if [ "${os_name}" != "Linux" ]; then
        log_error "MQ Launcher поддерживает только Linux (обнаружено: ${os_name})."
        return 1
    fi

    case "${arch_name}" in
        x86_64|amd64)
            ;;
        *)
            log_error "MQ Launcher поддерживает архитектуру x86_64 (обнаружено: ${arch_name})."
            return 1
            ;;
    esac
}

detect_distro() {
    if [ -f /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        DISTRO_ID="${ID:-unknown}"
        DISTRO_LIKE="${ID_LIKE:-}"
    else
        DISTRO_ID="unknown"
        DISTRO_LIKE=""
    fi
}

check_dependencies_ldd() {
    target_bin="$1"

    if ! command -v ldd >/dev/null 2>&1; then
        log_warn "Утилита ldd не найдена в системе, проверка разделяемых библиотек пропущена."
        return 0
    fi

    missing="$(ldd "${target_bin}" 2>/dev/null | grep 'not found' || true)"
    if [ -z "${missing}" ]; then
        log_success "Все динамические системные библиотеки удовлетворены."
        return 0
    fi

    detect_distro
    log_warn "Обнаружены отсутствующие динамические библиотеки:"
    printf "%s\n\n" "${missing}" >&2

    echo "Рекомендуемые команды установки недостающих пакетов:" >&2
    case "${DISTRO_ID}" in
        arch|manjaro|endeavouros|cachyos|artix)
            echo "  sudo pacman -S --needed wayland libx11 libxcursor libxrandr libxi libxkbcommon mesa vulkan-icd-loader" >&2
            ;;
        ubuntu|debian|linuxmint|pop|kali|elementary)
            echo "  sudo apt update && sudo apt install -y libwayland-client0 libwayland-cursor0 libwayland-egl1 libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon0 libgl1 libvulkan1" >&2
            ;;
        fedora|rhel|centos|rocky|almalinux)
            echo "  sudo dnf install -y wayland-libs libX11 libXcursor libXrandr libXi libxkbcommon mesa-libGL vulkan-loader" >&2
            ;;
        *)
            case "${DISTRO_LIKE}" in
                *arch*)
                    echo "  sudo pacman -S --needed wayland libx11 libxcursor libxrandr libxi libxkbcommon mesa vulkan-icd-loader" >&2
                    ;;
                *debian*|*ubuntu*)
                    echo "  sudo apt update && sudo apt install -y libwayland-client0 libwayland-cursor0 libwayland-egl1 libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon0 libgl1 libvulkan1" >&2
                    ;;
                *fedora*|*rhel*)
                    echo "  sudo dnf install -y wayland-libs libX11 libXcursor libXrandr libXi libxkbcommon mesa-libGL vulkan-loader" >&2
                    ;;
                *)
                    echo "  Arch:   sudo pacman -S --needed wayland libx11 libxcursor libxrandr libxi libxkbcommon mesa vulkan-icd-loader" >&2
                    echo "  Debian: sudo apt install libwayland-client0 libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon0 libgl1 libvulkan1" >&2
                    echo "  Fedora: sudo dnf install wayland-libs libX11 libXcursor libXrandr libXi libxkbcommon mesa-libGL vulkan-loader" >&2
                    ;;
            esac
            ;;
    esac
    echo "" >&2

    return 1
}

check_java() {
    if command -v java >/dev/null 2>&1; then
        java_ver="$(java -version 2>&1 | head -n 1)"
        log_success "Java найдена: ${java_ver}"
    else
        detect_distro
        log_warn "Java (JRE/JDK) не обнаружена в PATH."
        echo "Для запуска Minecraft рекомендуется Java 21 (или 17 для старых версий):" >&2
        case "${DISTRO_ID}" in
            arch|manjaro|endeavouros|cachyos|artix)
                echo "  sudo pacman -S --needed jre21-openjdk" >&2
                ;;
            ubuntu|debian|linuxmint|pop|kali)
                echo "  sudo apt update && sudo apt install -y openjdk-21-jre" >&2
                ;;
            fedora|rhel|centos|rocky|almalinux)
                echo "  sudo dnf install -y java-21-openjdk" >&2
                ;;
            *)
                echo "  Arch:   sudo pacman -S jre21-openjdk" >&2
                echo "  Debian: sudo apt install openjdk-21-jre" >&2
                echo "  Fedora: sudo dnf install java-21-openjdk" >&2
                ;;
        esac
        echo "" >&2
    fi
}

calc_sha256() {
    file_path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${file_path}" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "${file_path}" | awk '{print $1}'
    else
        log_error "В системе не найдены утилиты sha256sum или shasum для проверки хэша."
        return 1
    fi
}

verify_checksum() {
    file_path="$1"
    sums_file="$2"
    file_name="$(basename "${file_path}")"

    expected_hash="$(grep -E "(^|[[:space:]])${file_name}\$" "${sums_file}" | head -n1 | awk '{print $1}')"
    if [ -z "${expected_hash}" ]; then
        log_error "Файл ${file_name} отсутствует в контрольном списке ${sums_file}."
        return 1
    fi

    actual_hash="$(calc_sha256 "${file_path}")"
    if [ "${actual_hash}" != "${expected_hash}" ]; then
        log_error "Несовпадение SHA-256 для ${file_name}!"
        printf "  Ожидалось: %s\n" "${expected_hash}" >&2
        printf "  Получено:  %s\n" "${actual_hash}" >&2
        return 1
    fi

    short_hash="$(printf "%.16s" "${actual_hash}")"
    log_success "Хэш SHA-256 проверен (${short_hash}...)."
    return 0
}

verify_minisign_signature() {
    message_file="$1"
    sig_file="$2"
    pubkey="$3"

    if ! command -v minisign >/dev/null 2>&1; then
        log_info "Утилита minisign не найдена, пропуск проверки цифровой подписи (SHA-256 проверка активна)."
        return 0
    fi

    if [ ! -f "${sig_file}" ]; then
        log_info "Файл подписи ${sig_file} отсутствует в релизе, пропуск Minisign проверки."
        return 0
    fi

    if [ -z "${pubkey}" ]; then
        log_info "Публичный ключ Minisign не задан, пропуск криптографической проверки подписи."
        return 0
    fi

    log_step "Проверка криптографической подписи Minisign..."
    if minisign -Vm "${message_file}" -x "${sig_file}" -P "${pubkey}" >/dev/null 2>&1; then
        log_success "Цифровая подпись Minisign успешно подтверждена."
        return 0
    else
        log_error "Криптографическая проверка подписи Minisign НЕ ПРОЙДЕНА!"
        return 1
    fi
}

perform_uninstall() {
    bin_dir="$1"
    app_dir="$2"
    icon_dir="$3"
    dry_run="$4"

    target_bin="${bin_dir}/mq"
    target_desktop="${app_dir}/mq.desktop"
    target_icon="${icon_dir}/mq.svg"

    echo "================================================================================"
    printf "         %bMQ Launcher — Удаление компонентов%b\n" "${BOLD}" "${RESET}"
    echo "================================================================================"

    if [ "${dry_run}" -eq 1 ]; then
        log_info "[DRY-RUN] Будут удалены следующие файлы:"
        printf "  - %s\n" "${target_bin}"
        printf "  - %s\n" "${target_desktop}"
        printf "  - %s\n" "${target_icon}"
        log_success "[DRY-RUN] Пользовательские данные (~/.local/share/mq, ~/.config/mq) будут сохранены."
        return 0
    fi

    removed_count=0
    for f in "${target_bin}" "${target_desktop}" "${target_icon}"; do
        if [ -f "${f}" ]; then
            rm -f "${f}"
            log_info "Удален: ${f}"
            removed_count=$((removed_count + 1))
        fi
    done

    # Update desktop database & icon cache
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${app_dir}" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${icon_dir%/scalable/apps}" 2>/dev/null || true
    fi

    echo ""
    log_success "MQ Launcher успешно деинсталлирован (${removed_count} файлов удалено)."
    log_info "ВАЖНО: Ваши данные Minecraft, моды, инстансы и профили сохранены в:"
    printf "  %s\n" "${XDG_DATA_HOME:-${HOME}/.local/share}/mq"
    printf "  %s\n" "${XDG_CONFIG_HOME:-${HOME}/.config}/mq"
    return 0
}

main() {
    init_colors

    version="${MQ_VERSION:-latest}"
    uninstall_mode=0
    dry_run=0
    skip_deps=0
    minisign_pubkey="${MQ_MINISIGN_PUBKEY:-}"
    repo_user="${MQ_REPO_USER:-MrQuster}"
    repo_name="${MQ_REPO_NAME:-mq}"
    custom_install_dir="${MQ_INSTALL_DIR:-}"
    custom_base_url="${MQ_RELEASE_BASE_URL:-}"

    # Parse arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            -v|--version)
                if [ $# -lt 2 ]; then
                    log_error "Опция $1 требует значения (тег версии, например v2.2.0)."
                    exit 1
                fi
                version="$2"
                shift 2
                ;;
            --version=*)
                version="${1#*=}"
                shift
                ;;
            -u|--uninstall)
                uninstall_mode=1
                shift
                ;;
            -n|--dry-run)
                dry_run=1
                shift
                ;;
            --install-dir)
                if [ $# -lt 2 ]; then
                    log_error "Опция $1 требует пути к директории."
                    exit 1
                fi
                custom_install_dir="$2"
                shift 2
                ;;
            --install-dir=*)
                custom_install_dir="${1#*=}"
                shift
                ;;
            --minisign-key)
                if [ $# -lt 2 ]; then
                    log_error "Опция $1 требует значения публичного ключа."
                    exit 1
                fi
                minisign_pubkey="$2"
                shift 2
                ;;
            --minisign-key=*)
                minisign_pubkey="${1#*=}"
                shift
                ;;
            --skip-deps)
                skip_deps=1
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                log_error "Неизвестный параметр: $1 (используйте --help для справки)"
                exit 1
                ;;
        esac
    done

    # Destination directories
    bin_dir="${custom_install_dir:-${HOME}/.local/bin}"
    data_dir="${XDG_DATA_HOME:-${HOME}/.local/share}"
    app_dir="${data_dir}/applications"
    icon_dir="${data_dir}/icons/hicolor/scalable/apps"

    if [ "${uninstall_mode}" -eq 1 ]; then
        perform_uninstall "${bin_dir}" "${app_dir}" "${icon_dir}" "${dry_run}"
        exit 0
    fi

    echo "================================================================================"
    printf "         %bMQ Launcher — Solid Edition (1-Command Installer)%b\n" "${BOLD}" "${RESET}"
    echo "================================================================================"

    # 1. OS & Architecture preflight
    check_os_arch

    # Determine Base URL
    if [ -n "${custom_base_url}" ]; then
        base_url="${custom_base_url}"
    elif [ "${version}" = "latest" ]; then
        base_url="https://github.com/${repo_user}/${repo_name}/releases/latest/download"
    else
        # Normalize version: if "2.2.0" without "v", prepend "v" if tag style
        case "${version}" in
            v*) tag_ver="${version}" ;;
            *)  tag_ver="v${version}" ;;
        esac
        base_url="https://github.com/${repo_user}/${repo_name}/releases/download/${tag_ver}"
    fi

    target_bin_path="${bin_dir}/mq"
    target_desktop_path="${app_dir}/mq.desktop"
    target_icon_path="${icon_dir}/mq.svg"

    if [ "${dry_run}" -eq 1 ]; then
        log_info "[DRY-RUN] Будет выполнена установка со следующими параметрами:"
        printf "  Версия:          %s\n" "${version}"
        printf "  Базовый URL:     %s\n" "${base_url}"
        printf "  Бинарный файл:   %s\n" "${target_bin_path}"
        printf "  Ярлык меню:      %s\n" "${target_desktop_path}"
        printf "  Иконка:          %s\n" "${target_icon_path}"
        check_java || true
        log_success "[DRY-RUN] Симуляция успешно завершена (изменения на диск не внесены)."
        exit 0
    fi

    # Create temporary staging directory
    tmp_dir="$(mktemp -d -t mq-install-XXXXXX)"
    cleanup() {
        rm -rf "${tmp_dir}"
    }
    trap cleanup EXIT INT TERM

    log_step "Загрузка контрольных сумм и проверка целостности..."
    sums_file="${tmp_dir}/SHA256SUMS"
    if ! curl -fsSL "${base_url}/SHA256SUMS" -o "${sums_file}" 2>/dev/null; then
        log_error "Не удалось загрузить SHA256SUMS по адресу: ${base_url}/SHA256SUMS"
        exit 1
    fi

    # Minisign verification if signature and key exist
    sig_file="${tmp_dir}/SHA256SUMS.minisig"
    if curl -fsSL "${base_url}/SHA256SUMS.minisig" -o "${sig_file}" 2>/dev/null; then
        verify_minisign_signature "${sums_file}" "${sig_file}" "${minisign_pubkey}"
    elif command -v minisign >/dev/null 2>&1; then
        log_info "Файл SHA256SUMS.minisig не найден в релизе; используется проверка SHA-256."
    fi

    # Select payload: prefer mq-portable-linux-x86_64.tar.gz, fallback to mq-linux-x86_64
    payload_name=""
    if grep -q "mq-portable-linux-x86_64.tar.gz" "${sums_file}"; then
        payload_name="mq-portable-linux-x86_64.tar.gz"
    elif grep -q "mq-linux-x86_64" "${sums_file}"; then
        payload_name="mq-linux-x86_64"
    elif grep -E "(^|[[:space:]])mq\$" "${sums_file}"; then
        payload_name="mq"
    else
        log_error "Подходящий исполняемый файл для Linux x86_64 не найден в SHA256SUMS."
        exit 1
    fi

    payload_file="${tmp_dir}/${payload_name}"
    log_step "Скачивание ${payload_name}..."
    if ! curl -fL --progress-bar -o "${payload_file}" "${base_url}/${payload_name}"; then
        log_error "Ошибка при скачивании ${payload_name} по адресу: ${base_url}/${payload_name}"
        exit 1
    fi

    # Verify SHA-256
    verify_checksum "${payload_file}" "${sums_file}"

    # Extract or prepare binaries and assets
    staged_bin="${tmp_dir}/mq"
    staged_svg="${tmp_dir}/mq.svg"
    staged_desktop="${tmp_dir}/mq.desktop"

    case "${payload_name}" in
        *.tar.gz|*.tgz)
            log_step "Распаковка архива ${payload_name}..."
            tar -xzf "${payload_file}" -C "${tmp_dir}"
            # Locate extracted files (might be inside subfolder mq-portable-linux-x86_64)
            found_bin="$(find "${tmp_dir}" -maxdepth 2 -type f -name "mq" ! -name "*.tar.gz" | head -n1)"
            if [ -n "${found_bin}" ]; then
                cp "${found_bin}" "${staged_bin}"
            fi
            found_svg="$(find "${tmp_dir}" -maxdepth 2 -type f -name "mq.svg" | head -n1)"
            if [ -n "${found_svg}" ]; then
                cp "${found_svg}" "${staged_svg}"
            fi
            found_desktop="$(find "${tmp_dir}" -maxdepth 2 -type f -name "mq.desktop" | head -n1)"
            if [ -n "${found_desktop}" ]; then
                cp "${found_desktop}" "${staged_desktop}"
            fi
            ;;
        *)
            cp "${payload_file}" "${staged_bin}"
            ;;
    esac

    if [ ! -f "${staged_bin}" ]; then
        log_error "Исполняемый файл mq не найден в распакованном дистрибутиве."
        exit 1
    fi
    chmod 755 "${staged_bin}"

    # Fetch missing desktop / icon assets if standalone binary
    raw_base="https://raw.githubusercontent.com/${repo_user}/${repo_name}/main"
    if [ ! -f "${staged_svg}" ]; then
        curl -fsSL "${base_url}/mq.svg" -o "${staged_svg}" 2>/dev/null || \
        curl -fsSL "${raw_base}/packaging/mq.svg" -o "${staged_svg}" 2>/dev/null || true
    fi
    if [ ! -f "${staged_desktop}" ]; then
        curl -fsSL "${base_url}/mq.desktop" -o "${staged_desktop}" 2>/dev/null || \
        curl -fsSL "${raw_base}/packaging/mq.desktop" -o "${staged_desktop}" 2>/dev/null || true
    fi

    # Library compatibility check (ldd)
    if [ "${skip_deps}" -eq 0 ]; then
        log_step "Проверка совместимости системных библиотек..."
        if ! check_dependencies_ldd "${staged_bin}"; then
            log_error "Не выполнены требования к библиотекам. Установите пакеты выше или используйте --skip-deps."
            exit 1
        fi
    fi

    # Check Java runtime
    check_java || true

    # Install into target directories
    log_step "Установка компонентов в пользовательское окружение..."
    mkdir -p "${bin_dir}" "${app_dir}" "${icon_dir}"

    # Install binary
    install -m 755 "${staged_bin}" "${target_bin_path}"
    log_success "Исполняемый файл установлен в ${target_bin_path}"

    # If ~/.cargo/bin exists and is in PATH, create symlink to avoid collisions with /usr/local/bin
    cargo_bin="${HOME}/.cargo/bin"
    if [ -d "${cargo_bin}" ] && [ "${bin_dir}" != "${cargo_bin}" ]; then
        case ":${PATH}:" in
            *":${cargo_bin}:"*)
                ln -sf "${target_bin_path}" "${cargo_bin}/mq" 2>/dev/null || true
                ;;
        esac
    fi

    # Install icon
    if [ -f "${staged_svg}" ]; then
        install -m 644 "${staged_svg}" "${target_icon_path}"
        log_success "Иконка установлена в ${target_icon_path}"
    fi

    # Install desktop shortcut
    if [ -f "${staged_desktop}" ]; then
        # Ensure desktop file points directly to the installed binary
        sed -e "s|^Exec=.*|Exec=${target_bin_path} %u|" \
            -e "s|^TryExec=.*|TryExec=${target_bin_path}|" \
            "${staged_desktop}" > "${tmp_dir}/mq.desktop.processed"
        install -m 644 "${tmp_dir}/mq.desktop.processed" "${target_desktop_path}"
        log_success "Ярлык меню приложений установлен в ${target_desktop_path}"
    fi

    # Update desktop and icon databases
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${app_dir}" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${icon_dir%/scalable/apps}" 2>/dev/null || true
    fi

    # Check PATH
    in_path=0
    case ":${PATH}:" in
        *":${bin_dir}:"*)
            in_path=1
            ;;
    esac

    echo ""
    echo "================================================================================"
    printf " %bПОЗДРАВЛЯЕМ! MQ LAUNCHER УСПЕШНО УСТАНОВЛЕН!%b\n" "${GREEN}${BOLD}" "${RESET}"
    echo "================================================================================"
    printf " %bИсполняемый файл:%b  %s\n" "${CYAN}" "${RESET}" "${target_bin_path}"
    printf " %bКоманда запуска:%b   %bmq%b (или %s)\n" "${CYAN}" "${RESET}" "${BOLD}" "${RESET}" "${target_bin_path}"
    printf " %bЯрлык меню:%b        %s\n" "${CYAN}" "${RESET}" "${target_desktop_path}"
    printf " %bИконка:%b            %s\n" "${CYAN}" "${RESET}" "${target_icon_path}"
    printf " %bУдаление:%b          sh %s/install.sh --uninstall\n" "${CYAN}" "${RESET}" "${bin_dir}"

    if [ "${in_path}" -eq 0 ]; then
        echo "--------------------------------------------------------------------------------"
        log_warn "Директория '${bin_dir}' не найдена в системной переменной PATH."
        echo "Чтобы запускать команду 'mq' из любого терминала, добавьте строку в ~/.bashrc или ~/.zshrc:"
        printf "  %bexport PATH=\"%s:\$PATH\"%b\n" "${BOLD}" "${bin_dir}" "${RESET}"
    fi
    echo "================================================================================"
    echo ""
}

main "$@"
