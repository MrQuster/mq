# Agent 09: Security Hardening & Threat Model

- **Zone:** `docs/SECURITY.md`, `docs/THREAT_MODEL.md`, `docs/PRIVACY.md`, `deny.toml`, process spawning audit, archive security.
- **Goals:**
  1. Documentation:
     - `SECURITY.md`: Vulnerability reporting process, PGP/minisign public keys, private reporting via GitHub Security Advisories.
     - `docs/THREAT_MODEL.md`: Threat vectors (hostile modpacks, MITM, forged updates, rogue processes, path traversal).
     - `docs/PRIVACY.md`: Strict zero-telemetry policy, exhaustive enumeration of all network hosts contacted (Mojang, Modrinth, CurseForge, Adoptium, Ely.by, Microsoft, GitHub).
  2. Cargo tooling:
     - `cargo deny` configuration in `deny.toml` (licenses, advisories, bans, sources).
     - Dependabot configuration in `.github/dependabot.yml`.
  3. Process & Log Audits:
     - Enforce `Command::new(binary).args(args)` directly everywhere; zero shell evaluation.
     - Redact all tokens, codes, `code_verifier`, and `state` across logs, diagnostics, `--dry-run`, and crash dumps.
     - Modpack import guard: ignore any pre/post launch commands embedded inside imported packs.
  4. Archive & Network Security:
     - Zip-slip, symlink escape, and zip-bomb validation across all archive extraction logic.
  5. Code safety:
     - Add `#![forbid(unsafe_code)]` in all internal crates where possible; isolate necessary FFI in documented modules.
     - File permission enforcement (`0600` for configs and tokens).
     - Windows DLL hijacking mitigation: prevent loading DLLs from current working directory.
  6. Automated security test suite.
