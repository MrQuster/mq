---
name: mq-minecraft-meta
description: Minecraft version manifests, Java requirements, 26.x year.release scheme, loader APIs
---

# Minecraft Meta & Loader Guidelines

## Version Manifest Schema
- Modern versions use the `26.x` year.release scheme (e.g. `26.1`, `26.3`).
- **Never** parse or sort Minecraft versions as strict `1.x.y` semver strings.
- Sort versions chronologically by `releaseTime` from the official Mojang version manifest (`https://piston-meta.mojang.com/mc/game/version_manifest_v2.json`).
- Distinguish types: `release`, `snapshot`, `old_beta`, `old_alpha`.

## Java Version Determination
- Extract required Java major version from `version.json` field `javaVersion.majorVersion`.
- If missing or `<= 8`, default to Java 8. For modern versions (1.17 -> 16, 1.18..1.20.4 -> 17, 1.20.5+ / 26.x -> 21).
- Automatically download portable OpenJDK (Adoptium Temurin / Azul Zulu) if requested runtime is not installed locally.

## Loader APIs
- **Fabric**: `https://meta.fabricmc.net/v2/versions/loader/{game_version}`
- **Quilt**: `https://meta.quiltmc.org/v3/versions/loader/{game_version}`
- **Forge / NeoForge**: Query compatible builds dynamically; do not hardcode static matrices.
