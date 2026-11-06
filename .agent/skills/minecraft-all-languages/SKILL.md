---
name: minecraft-all-languages
description: Protocol for complete localization of the launcher and Minecraft game client (80+ locales, assetIndex priority, options.txt synchronization).
---

# Minecraft All-Languages Engine Protocol

## 1. Asset Index Language Objects
- Minecraft language files are indexed under `assets/indexes/<asset_index>.json`.
- Each language file is mapped by `minecraft/lang/<code_lang>.json` (e.g. `uk_ua.json`, `ru_ru.json`, `pl_pl.json`, etc.).
- Content hash SHA1 maps to Mojang resource URL: `https://resources.download.minecraft.net/<first_2_hex>/<sha1_hash>`.
- Language objects must be prioritized in the download DAG during installation / verification.

## 2. Automated options.txt Synchronization
- Before JVM launch (`mq-launch`), inspect `<instance_dir>/options.txt`.
- Read existing properties line by line to preserve user custom controls, FOV, render distance, etc.
- If `lang:<locale>` exists, update it to `<selected_mc_locale>`. If missing, append `lang:<selected_mc_locale>`.
- Write back atomically via tempfile rename.

## 3. Supported Locales (80+ Locales)
- Support full locale list from Mojang assets with human-readable native and English names (e.g., `uk_ua` -> Українська, `ru_ru` -> Русский, `en_us` -> English (US), etc.).
