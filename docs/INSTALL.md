# Руководство по установке (INSTALL)

В этом документе приведены подробные инструкции по установке **MQ Launcher** на различных дистрибутивах Linux и операционных системах Windows 10/11 x64.

---

## Варианты установки

| Платформа | Рекомендуемый метод | Альтернативные варианты |
|---|---|---|
| **Linux (Все дистрибутивы)** | Скрипт установки `curl \| sh` | Портативный `.tar.gz`, сборка из исходников |
| **Arch Linux / Manjaro** | PKGBUILD / `makepkg` | Скрипт установки `curl \| sh`, портативный `.tar.gz` |
| **Ubuntu / Debian / Mint** | Скрипт установки `curl \| sh` | Портативный `.tar.gz`, сборка через `cargo` |
| **Fedora / RHEL** | Скрипт установки `curl \| sh` | Портативный `.tar.gz`, сборка через `cargo` |
| **Windows 10 / 11 x64** | PowerShell `irm \| iex` | Inno Setup (`.exe`), портативный `.zip`, Winget, Scoop |

---

## 1. Универсальная установка на Linux

### Способ 1: Автоматический скрипт установки (Рекомендуется)

Скрипт загружает актуальный релиз, проверяет контрольные суммы SHA-256, устанавливает бинарный файл в `~/.local/bin/mq`, регистрирует `.desktop` файл в `~/.local/share/applications/` и добавляет векторную иконку:

```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
```

Для установки конкретной версии:
```bash
curl -fsSL https://github.com/MrQuster/mq/releases/download/v2.2.0/install.sh | sh
```

### Способ 2: Портативный архив (Без интеграции в систему)

Подходит для запуска без установки или на машинах с ограниченными правами:

```bash
# 1. Загрузите архив релиза
curl -LO https://github.com/MrQuster/mq/releases/download/v2.2.0/mq-linux-x86_64.tar.gz

# 2. Распакуйте архив
tar -xzf mq-linux-x86_64.tar.gz

# 3. Запустите лаунчер
./mq
```

---

## 2. Установка по дистрибутивам Linux

### Arch Linux / Manjaro

#### Вариант A: Локальная сборка пакета через PKGBUILD
В репозитории подготовлен официальный манифест `PKGBUILD`:

```bash
git clone https://github.com/MrQuster/mq.git
cd mq/packaging
makepkg -si
```

#### Вариант B: Быстрая локальная установка
```bash
git clone https://github.com/MrQuster/mq.git
cd mq
./scripts/install-local.sh
```

---

### Ubuntu / Debian / Linux Mint

Перед запуском убедитесь, что установлены базовые системные библиотеки (X11, Wayland, OpenGL):

```bash
sudo apt-get update
sudo apt-get install -y curl ca-certificates libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon0 libwayland-client0 libgl1
```

Установите лаунчер одной командой:
```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
```

Убедитесь, что каталог `~/.local/bin` включен в переменную `$PATH`. Если команда `mq` не найдена в терминале, добавьте следующую строку в `~/.bashrc` или `~/.zshrc`:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

---

### Fedora / RHEL

Установите необходимые библиотеки:
```bash
sudo dnf install -y curl ca-certificates libX11 libXcursor libXrandr libXi libxkbcommon wayland-client mesa-libGL
```

Установите лаунчер:
```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
```

---

## 3. Установка на Windows 10 / 11 x64

### Способ 1: PowerShell скрипт в 1 команду (Рекомендуется)

Не требует прав администратора. Лаунчер устанавливается в профиль пользователя (`%LOCALAPPDATA%\Programs\MQ`), создает ярлыки в меню «Пуск» и на рабочем столе, а также добавляет путь в переменную среды `PATH`:

```powershell
irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex
```

Для установки конкретной версии:
```powershell
& ([scriptblock]::Create((irm https://github.com/MrQuster/mq/releases/download/v2.2.0/install.ps1))) -Version 2.2.0
```

### Способ 2: Классический инсталлятор Inno Setup
1. Скачайте [`MQ-Setup-2.2.0.exe`](https://github.com/MrQuster/mq/releases/download/v2.2.0/MQ-Setup-2.2.0.exe).
2. Запустите установщик и следуйте подсказкам мастера.
3. Доступен режим тихой установки для скриптов автоматизации:
   ```cmd
   MQ-Setup-2.2.0.exe /VERYSILENT /NORESTART
   ```

### Способ 3: Портативная версия (Portable ZIP)
1. Скачайте [`mq-windows-x86_64-portable.zip`](https://github.com/MrQuster/mq/releases/download/v2.2.0/mq-windows-x86_64-portable.zip).
2. Распакуйте архив в любую удобную папку или на съемный USB-накопитель.
3. Наличие маркера `portable.flag` указывает лаунчеру изолировать все игровые данные, конфигурацию и логи в локальной папке `data/` рядом с `mq.exe`.

### Способ 4: Пакетный менеджер Windows (winget)
```powershell
winget install MrQuster.MQ
```

### Способ 5: Scoop
```powershell
scoop install https://raw.githubusercontent.com/MrQuster/mq/main/packaging/scoop/mq.json
```

---

## 4. Верификация целостности и цифровой подписи

Официальные релизы MQ подписываются цифровой подписью **Minisign (Ed25519)** и сопровождаются аттестацией происхождения Sigstore.

### Публичный ключ Minisign:
```
untrusted comment: minisign public key: AB8B07260EB5C665
RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
```

### Проверка подписи контрольных сумм:
```bash
# Загрузка архива, манифеста и подписи
curl -LO https://github.com/MrQuster/mq/releases/download/v2.2.0/SHA256SUMS
curl -LO https://github.com/MrQuster/mq/releases/download/v2.2.0/SHA256SUMS.minisig

# Проверка подписи манифеста
minisign -Vm SHA256SUMS -P RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt

# Проверка хэша бинарного файла
sha256sum -c SHA256SUMS --ignore-missing
```

### Проверка аттестации цепочки поставок через GitHub CLI:
```bash
gh attestation verify mq-linux-x86_64 --repo MrQuster/mq
```

---

## 5. Проверка после установки

После завершения установки выполните диагностическую команду:

```bash
mq doctor
```

Команда выполнит автоматическую проверку окружения:
- Корректность путей к данным и конфигурации.
- Доступность системной среды Java (или предложит автозагрузку Adoptium Temurin).
- Состояние графической подсистемы и драйверов.
- Связность с серверами Mojang, Modrinth и Ely.by.

---

## 6. Удаление

### Linux:
Если лаунчер был установлен через `install.sh`:
```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh -s -- --uninstall
```
Либо вручную:
```bash
rm -f ~/.local/bin/mq
rm -f ~/.local/share/applications/mq.desktop
rm -f ~/.local/share/icons/hicolor/scalable/apps/mq.svg
```

### Windows:
Если лаунчер был установлен через `install.ps1`:
```powershell
& ([scriptblock]::Create((irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1))) -Uninstall
```
Либо через «Параметры Windows» → «Установленные приложения» → «MQ Launcher» → «Удалить».
