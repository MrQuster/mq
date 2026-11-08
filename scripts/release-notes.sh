#!/usr/bin/env bash
# scripts/release-notes.sh — Генератор структурированных заметок релиза MQ
# Использование:
#   ./scripts/release-notes.sh --version <vX.Y.Z> [--title <Название>] [--assets-dir <дир>] [--output <файл>]

set -euo pipefail

VERSION=""
TITLE=""
ASSETS_DIR=""
OUTPUT_FILE=""
MINISIGN_PUBKEY="RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt"

usage() {
    cat << 'EOF'
Использование:
  ./scripts/release-notes.sh [ПАРАМЕТРЫ]

Параметры:
  -v, --version <версия>    Версия релиза (например: v2.3.0 или 2.3.0) [обязательно]
  -t, --title <заголовок>   Заголовок релиза (по умолчанию: "MQ <версия>")
  -d, --assets-dir <каталог> Каталог с готовыми артефактами релиза для расчёта хэшей
  -o, --output <файл>       Путь к выходному файлу (по умолчанию: стандартный вывод)
  -h, --help                Показать эту справку

Пример:
  ./scripts/release-notes.sh --version v2.3.0 --title "MQ v2.3.0 Solid Edition" --assets-dir dist/ -o release-notes.md
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--version)
            VERSION="$2"
            shift 2
            ;;
        -t|--title)
            TITLE="$2"
            shift 2
            ;;
        -d|--assets-dir)
            ASSETS_DIR="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_FILE="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Ошибка: неизвестный аргумент '$1'" >&2
            usage
            ;;
    esac
done

if [[ -z "${VERSION}" ]]; then
    echo "Ошибка: параметр --version является обязательным." >&2
    exit 1
fi

# Нормализуем формат тега (гарантируем префикс 'v')
if [[ "${VERSION}" != v* ]]; then
    TAG="v${VERSION}"
else
    TAG="${VERSION}"
fi

if [[ -z "${TITLE}" ]]; then
    TITLE="MQ Launcher ${TAG}"
fi

# Буфер вывода
buf=""
append() {
    buf+="$1"$'\n'
}

append "# ${TITLE}"
append ""
append "Быстрый модульный лаунчер Minecraft для Linux и Windows на Rust."
append ""
append "## Что нового"
append ""
append "- **Основные изменения:** Краткое описание ключевых нововведений."
append "- **Улучшения производительности:** Оптимизация запуска и сетевого стека."
append "- **Исправления ошибок:** Список исправленных проблем."
append "- **Безопасность:** Проверка целостности зависимостей и изоляция данных."
append ""
append "## Файлы для скачивания"
append ""
append "| Файл | Назначение / Платформа | Размер | Контрольная сумма SHA-256 |"
append "| :--- | :--- | :--- | :--- |"

has_minisig=false

if [[ -n "${ASSETS_DIR}" && -d "${ASSETS_DIR}" ]]; then
    for file in "${ASSETS_DIR}"/*; do
        if [[ -f "${file}" ]]; then
            fname=$(basename "${file}")
            # Пропускаем временные или скрытые файлы
            [[ "${fname}" == .* ]] && continue
            
            bytes=$(stat -c%s "${file}" 2>/dev/null || stat -f%z "${file}" 2>/dev/null || echo "0")
            if [[ ${bytes} -ge 1048576 ]]; then
                size_str="$(awk "BEGIN {printf \"%.2f MB\", ${bytes}/1048576}")"
            elif [[ ${bytes} -ge 1024 ]]; then
                size_str="$(awk "BEGIN {printf \"%.2f KB\", ${bytes}/1024}")"
            else
                size_str="${bytes} B"
            fi
            
            sha256=$(sha256sum "${file}" | awk '{print $1}')
            
            desc="Бинарный файл релиза"
            case "${fname}" in
                *.tar.gz) desc="Портативный архив Linux (x86_64)" ;;
                *.zip) desc="Портативный архив Windows (x86_64)" ;;
                *.exe)
                    if [[ "${fname}" == *"Setup"* ]]; then
                        desc="Графический инсталлятор Windows"
                    else
                        desc="Автономный исполняемый файл Windows (x86_64)"
                    fi
                    ;;
                install.sh) desc="Скрипт быстрой установки Linux" ;;
                install.ps1) desc="Скрипт быстрой установки Windows" ;;
                SHA256SUMS) desc="Манифест контрольных сумм SHA-256" ;;
                SHA256SUMS.minisig)
                    desc="Цифровая подпись Minisign Ed25519"
                    has_minisig=true
                    ;;
                *.desktop) desc="Файл интеграции Linux Desktop Entry" ;;
                *.svg) desc="Векторная иконка приложения" ;;
                PKGBUILD) desc="Скрипт сборки пакета Arch Linux" ;;
                mq|mq-linux-*) desc="Автономный бинарный файл Linux (x86_64)" ;;
            esac
            
            append "| \`${fname}\` | ${desc} | ${size_str} | \`${sha256}\` |"
        fi
    done
else
    append "| \`mq-linux-x86_64.tar.gz\` | Портативный архив Linux (x86_64) | — | \`...\` |"
    append "| \`mq-windows-x86_64-portable.zip\` | Портативный архив Windows (x86_64) | — | \`...\` |"
    append "| \`install.sh\` | Скрипт быстрой установки Linux | — | \`...\` |"
    append "| \`install.ps1\` | Скрипт быстрой установки Windows | — | \`...\` |"
    append "| \`SHA256SUMS\` | Манифест контрольных сумм SHA-256 | — | \`...\` |"
    append "| \`SHA256SUMS.minisig\` | Цифровая подпись Minisign Ed25519 | — | \`...\` |"
    has_minisig=true
fi

append ""
append "## Проверка целостности и подписи"
append ""
append "### Проверка контрольной суммы SHA-256"
append ""
append "**Linux / macOS:**"
append "\`\`\`bash"
append "# Проверка всех файлов по манифесту SHA256SUMS:"
append "sha256sum -c SHA256SUMS"
append ""
append "# Или проверка конкретного файла:"
append "sha256sum <имя_файла>"
append "\`\`\`"
append ""
append "**Windows (PowerShell):**"
append "\`\`\`powershell"
append "Get-FileHash <имя_файла> -Algorithm SHA256"
append "\`\`\`"

if [[ "${has_minisig}" == "true" ]]; then
    append ""
    append "### Проверка цифровой подписи (Minisign Ed25519)"
    append ""
    append "Официальный публичный ключ проекта:"
    append "\`\`\`text"
    append "${MINISIGN_PUBKEY}"
    append "\`\`\`"
    append ""
    append "Команда верификации:"
    append "\`\`\`bash"
    append "minisign -Vm SHA256SUMS -x SHA256SUMS.minisig -P ${MINISIGN_PUBKEY}"
    append "\`\`\`"
fi

append ""
append "---"
append "**Полный журнал изменений:** https://github.com/MrQuster/mq/blob/main/CHANGELOG.md"

if [[ -n "${OUTPUT_FILE}" ]]; then
    echo -n "${buf}" > "${OUTPUT_FILE}"
    echo "Заметки релиза сохранены в '${OUTPUT_FILE}'"
else
    echo -n "${buf}"
fi
