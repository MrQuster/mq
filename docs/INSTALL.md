# Руководство по установке MQ (Installation Guide)

MQ — кроссплатформенный модульный лаунчер Minecraft для **Linux (x86_64)** и **Windows 10/11 (x64)**.

---

## ⚡ Быстрая установка

### 🐧 Linux (Arch, Ubuntu, Debian, Fedora, NixOS)

Установка актуального стабильного релиза с системной интеграцией (`~/.local/bin/mq`, иконка и desktop-файл):

```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
```

### 🪟 Windows 10 / 11 (PowerShell)

Установка в профиль пользователя (`%LOCALAPPDATA%\Programs\MQ`), не требующая прав администратора:

```powershell
irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex
```

---

## 📦 Пакетные менеджеры и установщики Windows

* **Windows Package Manager (winget):**
  ```powershell
  winget install MrQuster.MQ
  ```
* **Scoop:**
  ```powershell
  scoop install https://raw.githubusercontent.com/MrQuster/mq/main/packaging/scoop/mq.json
  ```
* **Inno Setup:** Скачайте инсталлятор `MQ-Setup-2.2.0.exe` со страницы [GitHub Releases](https://github.com/MrQuster/mq/releases/latest). Поддерживает тихий режим:
  ```powershell
  MQ-Setup-2.2.0.exe /VERYSILENT /NORESTART
  ```
* **Портативная версия (ZIP):** Распакуйте `mq-windows-x86_64-portable.zip` в любую папку или на USB-накопитель. Все данные сохраняются рядом с исполняемым файлом благодаря маркеру `portable.flag`.

---

## 🛠 Сборка из исходного кода

### Системные требования
- **Rust Toolchain:** 1.80 или новее (`rustup default stable`)
- **Linux зависимости:** OpenSSL / pkg-config, Wayland / X11 dev-библиотеки, libxkbcommon:
  ```bash
  # Arch Linux
  sudo pacman -S --needed base-devel git rustup libxkbcommon wayland

  # Ubuntu / Debian
  sudo apt-get update && sudo apt-get install -y build-essential libxkbcommon-dev libwayland-dev
  ```

### Сборка
```bash
git clone https://github.com/MrQuster/mq.git
cd mq

# Сборка оптимизированного релизного бинарника
cargo build --release

# Исполняемый файл доступен в target/release/mq
./target/release/mq --version
```

### Локальная установка на Linux
```bash
./scripts/install-local.sh
```

---

## 🔐 Проверка цифровой подписи (Minisign Ed25519)

Официальный публичный ключ релизов:
```
untrusted comment: minisign public key: AB8B07260EB5C665
RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
```

Проверка контрольных сумм:
```bash
minisign -Vm SHA256SUMS -P RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
sha256sum -c SHA256SUMS
```
