# BLOCKERS.md — Аудит окружения Arch Linux (Step 0) для MQ Minecraft Launcher

Статус: **Критических блокеров нет.** Среда полностью готова к сборке Rust-воркспейса, проверкам `clippy`/`rustfmt`, рендерингу GUI (Wayland/XWayland + аппаратный Vulkan/OpenGL), запуску современного Minecraft (1.17–1.21+) и работе с внешними API.

---

## 1. Фактические параметры среды

### Тулчейн Rust
- **Источник:** системный пакет Arch Linux `rust 1:1.99.1-1` (`/usr/bin/rustc`), `rustup` не установлен.
- `rustc`: `1.99.1 (58cf8f9c3 2026-10-13)`
- `cargo`: `1.99.1 (5f94df478 2026-08-27)`
- `clippy-driver`: `clippy 0.1.99`
- `rustfmt`: `1.10.0`
- **Примечание:** системные `cargo`/`rustc` игнорируют `rust-toolchain.toml` и используют уже установленные системные компоненты; файл `rust-toolchain.toml` добавлен для совместимости с CI и окружениями на базе `rustup`.

### CPU, память и телеметрия ОС
- **CPU:** `Intel(R) Core(TM) i5-2400S CPU @ 2.50GHz` (`x86_64`, 1 сокет, 4 физических ядра, 1 поток на ядро, `nproc = 4`, частоты 1600–3300 МГц).
- **ОЗУ / Swap:** `7.7 GiB` RAM (~`5.8 GiB` доступно), `3.8 GiB` Swap.
- **`/proc/stat`:** доступен для чтения без привилегий (агрегированная строка `cpu` и по-ядерные `cpu0`..`cpu3`).

### Дисплейная сессия и графика
- **Сессия:** Wayland (`XDG_SESSION_TYPE=wayland`, `WAYLAND_DISPLAY=wayland-1`, `XDG_CURRENT_DESKTOP=Hyprland`) + активный XWayland (`DISPLAY=:1`).
- **GPU:** дискретная видеокарта `AMD Radeon R9 M360 / Cape Verde PRO` (`0x1002:0x682b`, `4096 MB` выделенной VRAM).
- **Vulkan:** Instance `1.4.363`, устройство `AMD Radeon R9 M360 (RADV VERDE)`, драйвер `Mesa 26.2.6-arch1.1` (API `1.3.354`), ICD: `/usr/share/vulkan/icd.d/radeon_icd.json`. Поддерживаются `VK_KHR_wayland_surface`, `VK_KHR_xcb_surface`, `VK_KHR_xlib_surface`.
- **OpenGL:** `4.6 (Core / Compatibility Profile) Mesa 26.2.6-arch1.1` (`radeonsi`), `GLES 3.2`, аппаратное ускорение (`direct rendering: Yes`).

### Установленные среды Java (`/usr/lib/jvm`)
- `$JAVA_HOME`: не задана (лаунчер должен обнаруживать JVM через `/usr/lib/jvm/*`, `PATH` и пользовательский каталог рантаймов).
- Установленные JVM:
  - `/usr/lib/jvm/java-17-openjdk` (`jdk17-openjdk 17.0.21.0.3.u1-1`, выбрана по умолчанию `default` / `default-runtime`; также предоставляет виртуальный пакет `jre17-openjdk`) — покрывает Minecraft **1.17 – 1.20.4**.
  - `/usr/lib/jvm/java-21-openjdk` (`jdk21-openjdk 21.0.13.0.3.u1-1`) — покрывает Minecraft **1.20.5+ / 1.21+**.
  - `/usr/lib/jvm/java-27-openjdk` (`jre-openjdk 27.0.1.0.3.u1-1`).

### Системные и пользовательские шрифты (кириллица и моноширинные)
- Системные шрифты с поддержкой кириллицы в `/usr/share/fonts`:
  - Sans-serif: `/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf`, `/usr/share/fonts/gnu-free/FreeSans.otf`, `/usr/share/fonts/gsfonts/NimbusSans-Regular.otf`.
  - Monospace: `/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf`, `/usr/share/fonts/IosevkaNerdFont/IosevkaNerdFontMono-*.ttf`, `/usr/share/fonts/gnu-free/FreeMono.otf`, `/usr/share/fonts/gsfonts/NimbusMonoPS-Regular.otf`.
- Пользовательские шрифты в `~/.local/share/fonts`: `JetBrains Mono` (`JetBrainsMono-*.ttf`) и `Iosevka Nerd Font`.
- **Важный нюанс для UI:** пакеты `noto-fonts` (кроме Emoji/CJK) и `ttf-dejavu` не установлены (`NotoSans-Regular.ttf` и `DejaVuSans.ttf` отсутствуют). В GUI-слое нельзя полагаться на жёстко зашитый путь только к `NotoSans`/`DejaVu` — необходимо использовать системный резолвинг (`fontdb` / `fontconfig`) с fallback на `Adwaita Sans` / `FreeSans` и `Adwaita Mono` / `JetBrains Mono` / `Iosevka Nerd Font Mono`.

### Сетевая доступность
- `https://api.modrinth.com/v2/tag/loader` — **HTTP 200 OK** (валидный JSON, 29 загрузчиков).
- `https://authlib-injector.yushi.moe/artifact/latest.json` — **HTTP 200 OK** (валидный JSON с версией, URL и контрольными суммами).
- `https://index.crates.io/config.json` — **HTTP 200 OK**.

---

## 2. Аудит пакетов Arch Linux (`pacman -Q`)

### Установлены (19 из 23 проверенных позиций)
`base-devel`, `pkgconf`, `git`, `libxkbcommon`, `wayland`, `libx11`, `libxcb`, `libxi`, `libxcursor`, `libxrandr`, `vulkan-icd-loader`, `vulkan-tools`, `vulkan-radeon`, `mesa`, `openal`, `libpulse`, `jdk21-openjdk`, `jdk17-openjdk` (удовлетворяет и `jre17-openjdk`).

### Отсутствуют (опциональные / аппаратно-специфичные)
1. **`vulkan-swrast`** *(опционально)* — программный драйвер Vulkan (Lavapipe/llvmpipe).
   - **Неблокирующий обходной путь:** в системе активен аппаратный драйвер `vulkan-radeon` (Mesa RADV) и живая графическая сессия Wayland/XWayland. Для headless-тестов без GPU при необходимости программного OpenGL уже доступен `LIBGL_ALWAYS_SOFTWARE=1` через установленный `mesa`, либо тесты рендера выполняются на RADV.
2. **`jre8-openjdk`** *(опционально)* — среда выполнения Java 8 для старых версий Minecraft (`<= 1.16.5`, старые модпаки Forge 1.7.10 / 1.12.2).
   - **Неблокирующий обходной путь:** современные версии Minecraft полностью обеспечены системными `jdk17-openjdk` и `jdk21-openjdk`. Для старых версий лаунчер может автоматически скачивать портативную сборку Java 8 (Mojang / Adoptium) в пользовательский каталог данных (`~/.local/share/mq/runtimes/`) без прав `root`.
3. **`vulkan-intel`** *(не требуется)* — драйвер Vulkan для GPU Intel; в системе используется дискретная графика AMD Radeon.
4. **`nvidia-utils`** *(не требуется)* — проприетарные библиотеки NVIDIA; в системе нет GPU NVIDIA (установка не рекомендуется).

---

## 3. Команда для установки опциональных пакетов

Если потребуется локальный системный Java 8 (для тестов Minecraft `<= 1.16.5` без скачивания рантайма) и программный Vulkan-рендерер Lavapipe (для изолированных headless-прогонов):

```bash
sudo pacman -S --needed vulkan-swrast jre8-openjdk
```

*(Опционально для расширения набора стандартных системных шрифтов кириллицы можно добавить `noto-fonts ttf-dejavu`: `sudo pacman -S --needed vulkan-swrast jre8-openjdk noto-fonts ttf-dejavu`.)*
