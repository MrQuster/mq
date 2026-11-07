#!/usr/bin/env bash
# ==============================================================================
#  MQ Launcher — Local User-Space Installer & Manager
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

show_help() {
    cat << 'EOF'
MQ Local User-Space Installer

Использование:
  ./scripts/install-local.sh [ОПЦИИ]

Опции:
  -u, --uninstall           Удалить установленные файлы (бинарник, ярлык, иконку). Данные сохраняются!
  -n, --dry-run             Симуляция выполнения без изменения файлов на диске
      --install-dir <DIR>   Директория для установки бинарника (или через MQ_INSTALL_DIR)
  -h, --help                Показать эту справку и выйти

Переменные окружения:
  MQ_INSTALL_DIR            Путь установки бинарника (по умолчанию: ~/.local/bin)
  XDG_DATA_HOME             Базовый каталог данных XDG (по умолчанию: ~/.local/share)
EOF
}

main() {
    local uninstall_mode=0
    local dry_run=0
    local custom_install_dir="${MQ_INSTALL_DIR:-}"

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -u|--uninstall)
                uninstall_mode=1
                shift
                ;;
            -n|--dry-run)
                dry_run=1
                shift
                ;;
            --install-dir)
                custom_install_dir="$2"
                shift 2
                ;;
            --install-dir=*)
                custom_install_dir="${1#*=}"
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                echo "Неизвестный параметр: $1" >&2
                exit 1
                ;;
        esac
    done

    local bin_dir="${custom_install_dir:-${HOME}/.local/bin}"
    local data_dir="${XDG_DATA_HOME:-${HOME}/.local/share}"
    local app_dir="${data_dir}/applications"
    local icon_dir="${data_dir}/icons/hicolor/scalable/apps"

    local target_bin="${bin_dir}/mq"
    local target_desktop="${app_dir}/mq.desktop"
    local target_icon="${icon_dir}/mq.svg"

    if [[ "${uninstall_mode}" -eq 1 ]]; then
        echo "=== MQ: Удаление локально установленных компонентов ==="
        if [[ "${dry_run}" -eq 1 ]]; then
            echo "[DRY-RUN] Будут удалены файлы:"
            echo "  - ${target_bin}"
            echo "  - ${target_desktop}"
            echo "  - ${target_icon}"
            echo "[DRY-RUN] Данные пользователя (~/.local/share/mq) сохраняются."
            exit 0
        fi

        local removed=0
        for f in "${target_bin}" "${target_desktop}" "${target_icon}"; do
            if [[ -f "${f}" ]]; then
                rm -f "${f}"
                echo "Удален: ${f}"
                removed=$((removed + 1))
            fi
        done

        if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "${app_dir}" 2>/dev/null || true
        fi
        if command -v gtk-update-icon-cache >/dev/null 2>&1; then
            gtk-update-icon-cache -f -t "${data_dir}/icons/hicolor" 2>/dev/null || true
        fi

        echo "Удаление завершено (${removed} файлов удалено)."
        echo "Пользовательские данные в ${data_dir}/mq сохранены."
        exit 0
    fi

    echo "=== MQ Local User-space Installer ==="
    if [[ "${dry_run}" -eq 1 ]]; then
        echo "[DRY-RUN] Параметры установки:"
        echo "  Бинарный файл:  ${target_bin}"
        echo "  Ярлык меню:     ${target_desktop}"
        echo "  Иконка:         ${target_icon}"
        exit 0
    fi

    mkdir -p "${bin_dir}" "${app_dir}" "${icon_dir}"

    if [[ ! -f "${REPO_ROOT}/target/release/mq" ]]; then
        echo "Сборка релизного бинарника..."
        (cd "${REPO_ROOT}" && cargo build --release)
    fi

    echo "Установка бинарника в ${target_bin}..."
    install -m 755 "${REPO_ROOT}/target/release/mq" "${target_bin}"

    echo "Установка иконки в ${target_icon}..."
    install -m 644 "${REPO_ROOT}/packaging/mq.svg" "${target_icon}"

    echo "Установка desktop-ярлыка в ${target_desktop}..."
    sed -e "s|^Exec=.*|Exec=${target_bin} %u|" \
        -e "s|^TryExec=.*|TryExec=${target_bin}|" \
        "${REPO_ROOT}/packaging/mq.desktop" > "${target_desktop}"
    chmod 644 "${target_desktop}"

    # Update desktop and icon databases if tools exist
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${app_dir}" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${data_dir}/icons/hicolor" 2>/dev/null || true
    fi

    echo ""
    echo "Установка MQ успешно завершена!"
    echo "Бинарник: ${target_bin}"
    echo "Ярлык:    ${target_desktop}"
    echo "Убедитесь, что ${bin_dir} находится в вашей переменной PATH."
    echo "Запуск из терминала: mq"
    echo "Либо через меню приложений вашей среды (GNOME / KDE / Hyprland / Rofi / Wofi)."
}

main "$@"
