# Windows Package Manager (winget) Manifests for MQ

This directory contains manifests for submitting MQ Launcher to the official [Windows Package Manager Community Repository](https://github.com/microsoft/winget-pkgs).

## Directory Structure

- `manifests/m/MrQuster/MQ/<version>/`: Standard multi-file manifest format required by `winget-pkgs`:
  - `MrQuster.MQ.yaml`: Version manifest
  - `MrQuster.MQ.installer.yaml`: Installer details, silent switches, and SHA-256 hash
  - `MrQuster.MQ.locale.en-US.yaml`: English default metadata and descriptions
  - `MrQuster.MQ.locale.ru-RU.yaml`: Russian localized metadata
- `MrQuster.MQ.yaml`: Singleton manifest format for local testing

## Testing Locally

You can test installing MQ via winget with the local manifest:

```powershell
winget install --manifest packaging/winget/MrQuster.MQ.yaml
```

To validate the manifest format with `winget`:

```powershell
winget validate packaging/winget/manifests/m/MrQuster/MQ/2.2.0/
```

## Submitting a New Release Upstream

Use Microsoft's official `wingetcreate` tool:

```powershell
# Install wingetcreate
winget install Microsoft.WingetCreate

# Update and submit automatically
wingetcreate update MrQuster.MQ --version <NEW_VERSION> --urls https://github.com/MrQuster/mq/releases/download/v<NEW_VERSION>/MQ-Setup-<NEW_VERSION>.exe --token <GITHUB_PAT> --submit
```
