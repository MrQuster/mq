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

- **Isolated & Portable**: Game instances and configurations are saved in Scoop's `persist` directory (`data/`) and preserved across updates. Nothing is written to the Windows Registry.
- **Auto-Updates**: Automatically tracks GitHub Releases for new versions via Scoop's `checkver` and `autoupdate`.
- **Command-line & GUI**: Adds `mq` to the system `PATH` and registers a Windows Start Menu shortcut (`MQ`).

## Portable Archive Layout (`mq-windows-x86_64-portable.zip`)

The portable ZIP archive extracts into `mq-windows-x86_64-portable/` containing:

```text
mq-windows-x86_64-portable/
├── mq.exe          # Statically linked Windows x86_64 binary
├── portable.flag   # Empty marker file enabling portable mode (data/ next to mq.exe)
├── README.txt      # Quick-start and portable mode instructions
└── LICENSE         # MIT License
```

When `portable.flag` is present next to `mq.exe`, MQ stores all launcher data, instances, configs, and caches under `<exe_dir>\data\` (`data/config`, `data/cache`, `data/instances`, etc.) and writes nothing to the Windows Registry.

### Building the Portable Archive Locally (PowerShell)

```powershell
New-Item -ItemType Directory -Force dist/mq-windows-x86_64-portable | Out-Null
Copy-Item target/x86_64-pc-windows-msvc/release/mq.exe dist/mq-windows-x86_64-portable/
New-Item -ItemType File -Force dist/mq-windows-x86_64-portable/portable.flag | Out-Null
Copy-Item LICENSE dist/mq-windows-x86_64-portable/LICENSE
@"
MQ Launcher (Portable Edition)
==============================
All data, instances, configs, and caches are stored in the 'data\' folder next to mq.exe
because 'portable.flag' is present. Nothing is written to the Windows Registry.
"@ | Set-Content -Encoding UTF8 dist/mq-windows-x86_64-portable/README.txt
Compress-Archive -Path dist/mq-windows-x86_64-portable -DestinationPath dist/mq-windows-x86_64-portable.zip -Force
```
