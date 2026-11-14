<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/logo.svg">
  <source media="(prefers-color-scheme: light)" srcset="docs/assets/logo-light.svg">
  <img alt="MQ Launcher" src="docs/assets/logo.svg" width="104" height="104">
</picture>

# MQ Launcher

Кроссплатформенный лаунчер Minecraft на Rust с изолированными инстансами, пакетной установкой модов и настраиваемым интерфейсом.

[![Release](https://img.shields.io/github/v/release/MrQuster/mq?style=flat-square&color=4C9A82&labelColor=1F242D)](https://github.com/MrQuster/mq/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/MrQuster/mq/ci.yml?style=flat-square&color=3A4659&labelColor=1F242D&label=CI)](https://github.com/MrQuster/mq/actions/workflows/ci.yml)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-3A4659?style=flat-square&labelColor=1F242D)](#установка)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-3A4659?style=flat-square&labelColor=1F242D)](#сборка-из-исходников)

**Русский** · [English](README.en.md)

[Возможности](#возможности) · [Скриншоты](#скриншоты) · [Установка](#установка) · [Сборка из исходников](#сборка-из-исходников) · [Безопасность](#безопасность-и-приватность)

</div>

---

## Возможности

* **Изолированные инстансы** — отдельные каталоги (`mods/`, `resourcepacks/`, `shaderpacks/`, `config/`, `saves/`) для каждой сборки. Поддержка версий от `1.7.10` до `26.1` (`Релизы`, `Снапшоты`, `Beta`, `Alpha`), загрузчиков `Vanilla`, `Fabric`, `Quilt`, `Forge`, `NeoForge`, а также резервного копирования миров и анализа крашей.
* **Каталог Modrinth и CurseForge** — поиск модов, ресурс-паков, шейдеров и модпаков, локальный импорт `.mrpack` и `.zip`, проверка обновлений и управление установленными файлами прямо внутри инстанса (включение/выключение, поиск, фильтр по статусу, пакетное обновление).
* **Пакетная очередь установки** — выбирайте сразу несколько модов в очередь в каталоге: MQ подтянет и дедуплицирует общие обязательные зависимости и установит выбранный набор за один проход.
* **Настройка интерфейса без перезапуска** — 6 тем (`Matte Dark`, `Light Clean`, `Modrinth Emerald`, `Prism Classic Dark`, `Catppuccin Mocha`, `Nord`), спокойные пресеты акцентов и произвольный цвет, процедурный архитектурный фон или пользовательское изображение с регулировкой затемнения, а также настройка скруглений, прозрачности карточек, масштаба и шрифта.
* **Аккаунты и режимы входа** — Microsoft OAuth2 (Device Code Flow), Ely.by (OAuth2 + PKCE S256) с `authlib-injector` и локальный офлайн-режим с отображением скина.

---

## Скриншоты

<div align="center">

| Инстансы | Моды инстанса |
| :---: | :---: |
| ![Экран инстансов](docs/assets/screenshots/01-instances.png) | ![Моды инстанса](docs/assets/screenshots/02-detail-mods.png) |

| Каталог модов и сборки | Оформление (тёмная и светлая темы) |
| :---: | :---: |
| ![Каталог Modrinth и CurseForge](docs/assets/screenshots/04-catalog.png) | ![Настройки оформления](docs/assets/screenshots/06-settings-matte.png) |

| Аккаунты | Светлая тема |
| :---: | :---: |
| ![Менеджер аккаунтов](docs/assets/screenshots/05-accounts.png) | ![Светлая тема Light Clean](docs/assets/screenshots/07-settings-light.png) |

</div>

---

## Установка

Готовые сборки и контрольные суммы (`SHA256SUMS`, подпись Minisign `SHA256SUMS.minisig`) публикуются на странице [GitHub Releases](https://github.com/MrQuster/mq/releases).

### Windows (x64)

* **Установщик:** скачайте и запустите `MQ-Setup-<version>-x64.exe` со страницы релизов (или установите через PowerShell-скрипт `install.ps1` / манифесты `packaging/winget`, `packaging/scoop`).
* **Портативная версия:** распакуйте `mq-windows-x86_64-portable.zip` (файл `portable.flag` включает хранение всех данных рядом с исполняемым файлом).

### Linux (x86_64)

Установщик загружает релизный архив, проверяет SHA-256 и регистрирует приложение в меню:

```bash
curl -fsSL https://raw.githubusercontent.com/MrQuster/mq/main/install.sh | bash
```

Также доступны архив `mq-linux-x86_64.tar.gz`, отдельный бинарный файл `mq-linux-x86_64` и пакет для Arch Linux в `packaging/PKGBUILD`.

---

## Сборка из исходников

Требуется инструментарий Rust (`1.80+`, указан в `rust-toolchain.toml`).

```bash
# 1. Клонирование репозитория
git clone https://github.com/MrQuster/mq.git
cd mq

# 2. Запуск GUI из исходников
cargo run -p mq-cli -- ui

# 3. Релизная сборка
cargo build --release -p mq-cli

# 4. Проверка тестов и линтеров
./scripts/verify.sh
```

Исполняемый файл собирается в `target/release/mq` (`target\release\mq.exe` на Windows). Список системных зависимостей и инструкции для разработчиков описаны в [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Безопасность и приватность

| Механизм | Реализация в MQ Launcher |
|---|---|
| **Телеметрия** | Полностью отсутствует: нет аналитики, трекинга и отправки отчётов на сторонние серверы. |
| **Хранение токенов** | Системный Secret Service / Keyring на Linux, DPAPI / Credential Manager на Windows; локальный fallback-файл с правами `0600`. |
| **Авторизация Microsoft** | OAuth2 Device Code Flow во внешнем браузере без ввода пароля внутри лаунчера. |
| **Авторизация Ely.by** | OAuth2 + PKCE S256 через системный браузер и локальный callback `127.0.0.1:<port>/callback`. |
| **Защита архивов** | Фильтрация Zip-Slip и блокировка опасных путей при распаковке модпаков; игнорирование сторонних команд запуска. |
| **Целостность релизов** | Подпись Minisign Ed25519, GitHub OIDC build provenance и контрольные суммы SHA-256. |

Подробная документация: [SECURITY.md](docs/SECURITY.md) · [PRIVACY.md](docs/PRIVACY.md) · [THREAT_MODEL.md](docs/THREAT_MODEL.md) · [FAQ.md](docs/FAQ.md)

---

## Благодарности и правовая информация

MQ использует открытые спецификации и инструменты сообщества: [Modrinth](https://modrinth.com/), [Ely.by](https://ely.by/), [authlib-injector](https://github.com/yushijinhun/authlib-injector), [Fabric](https://fabricmc.net/), [Quilt](https://quiltmc.org/) и [NeoForge](https://neoforged.net/). Лицензии сторонних библиотек перечислены в [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

Автор: [MrQuster](https://github.com/MrQuster).

MQ Launcher является независимым проектом с открытым исходным кодом и не связан с Mojang Studios или Microsoft Corporation. Для входа на серверы с `online-mode=true` требуется лицензионная учётная запись Minecraft.
