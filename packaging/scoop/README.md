# Scoop Manifest for MQ Launcher

This directory contains the Scoop manifest `mq.json` for installing MQ Launcher via [Scoop](https://scoop.sh).

## User Installation

Users can install MQ Launcher using Scoop by adding the repository as a bucket or installing directly from the URL:

```powershell
# Option 1: Direct install from URL
scoop install https://raw.githubusercontent.com/MrQuster/mq/main/packaging/scoop/mq.json

# Option 2: Add custom bucket
scoop bucket add mq https://github.com/MrQuster/mq
scoop install mq
```

## Features

- **Isolated & Portable**: Game instances and configurations are saved in Scoop's `persist` directory and preserved across updates.
- **Auto-Updates**: Automatically tracks GitHub Releases for new versions via Scoop's `checkver` and `autoupdate`.
- **Command-line & GUI**: Adds `mq` to the system `PATH` and registers a Windows Start Menu shortcut.
