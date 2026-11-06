# MQ Overhaul Swarm Workflow

## Orchestration Plan

```mermaid
flowchart TD
    subgraph Phase 1: Core Audit & Architecture
        A1[ag-sub-01-bug-auditor]
        A2[ag-sub-02-liquid-glass-theme]
        A3[ag-sub-03-painter-icons-widgets]
    end

    subgraph Phase 2: Features & Customization
        A4[ag-sub-04-customization-studio]
        A5[ag-sub-05-launcher-i18n]
        A6[ag-sub-06-minecraft-lang-engine]
    end

    subgraph Phase 3: Modules & Integrations
        A7[ag-sub-07-modrinth-cf-browser]
        A8[ag-sub-08-auth-offline-elyby]
        A9[ag-sub-09-instances-log-viewer]
        A10[ag-sub-10-cpu-hud-telemetry]
    end

    subgraph Phase 4: Verification & Release
        A11[ag-sub-11-qa-validator]
        A12[ag-sub-12-github-release-packager]
    end

    Phase 1 --> Phase 2
    Phase 2 --> Phase 3
    Phase 3 --> Phase 4
```

### Steps:
1. **Phase 1**: Audit codebase for unwrap, deadlocks, and implement core Liquid Glass theme and vector painter icons.
2. **Phase 2**: Build full Customization Studio in Settings, integrate 10+ launcher languages and Minecraft 80+ locales engine.
3. **Phase 3**: Polish Modrinth/CurseForge browser, Offline/Ely.by skin rendering + authlib-injector, instance logs viewer, and smooth CPU HUD telemetry.
4. **Phase 4**: Complete QA sweep with `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, build portable bundle and publish GitHub release v1.1.0 with 1-command installer `install.sh`.
