---
name: mq-mods-shaders
description: Modrinth API, dependency resolution, lockfile, shaders engines and configs
---

# Mods, Shaders & Packs Specification

## Modrinth API
- Version details: `GET https://api.modrinth.com/v2/project/{id_or_slug}/version`
- Multiple file hashes: `POST https://api.modrinth.com/v2/version_files`
- Inspect `dependencies` field: filter by `dependency_type == "required"` (auto-install), `optional` (user toggle), `incompatible` (block launch).
- Match target `loaders` and `game_versions`.
- Filter `client_side` / `server_side` fields.

## Shader Engines
- **Fabric / Quilt**: Iris + Sodium.
- **NeoForge**: Iris.
- **Forge**: Oculus + Embeddium (or Rubidium on older Forge).
- Auto-detect when user imports a `.zip` shaderpack: prompt to install compatible engine if missing.
- Configuration: write selected shaderpack to `config/iris.properties` (`shaderPack=<name>.zip`, `enableShaders=true`).
- Inspect OpenGL version (`glxinfo -B`); warn if OpenGL < 3.3.
