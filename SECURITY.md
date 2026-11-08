# Security Policy — MQ Launcher

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 2.2.x   | Yes       |
| 2.1.x   | No        |
| < 2.1.0 | No        |

---

## Reporting a Vulnerability

We take the security of MQ Launcher very seriously. If you discover a vulnerability or security issue:

1. **GitHub Private Vulnerability Reporting (Preferred):**
   Navigate to the [Security Advisories](https://github.com/MrQuster/mq/security/advisories/new) tab on GitHub and submit a report privately.
2. **Direct Contact:**
   Alternatively, open a security advisory or contact `MrQuster` directly via GitHub: [https://github.com/MrQuster](https://github.com/MrQuster).

**Please do NOT submit public issues or PRs containing undisclosed security vulnerabilities.**
We aim to acknowledge reports within 48 hours and release patched builds promptly.

---

## Release Verification & Supply Chain Integrity

Every official release binary of MQ Launcher is built exclusively via automated GitHub Actions CI in an isolated runner environment, accompanied by:
- **Build Provenance Attestation** (`actions/attest-build-provenance`) signed via GitHub OIDC / Sigstore.
- **Cryptographic Checksums** (`SHA256SUMS`).
- **Minisign Ed25519 Digital Signature** (`SHA256SUMS.minisig`).

### Official Minisign Public Key
```
untrusted comment: minisign public key: AB8B07260EB5C665
RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
```

### Verifying a Release Signature
```bash
# On Linux using minisign:
minisign -Vm SHA256SUMS -P RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
sha256sum -c SHA256SUMS
```

### Verifying Build Provenance
```bash
gh attestation verify mq-linux-x86_64 --repo MrQuster/mq
```

---

## Security Guarantees & Architecture

1. **Zero Telemetry:** No user analytics, metrics, or telemetry are collected or transmitted.
2. **Credential Safety:** Secrets (Minecraft tokens, OAuth tokens, verification codes) are strictly isolated, stored with mode `0600` on Unix (or Windows Credential Manager / DPAPI on Windows), and filtered out from log outputs, stack traces, and `--dry-run` displays.
3. **Execution Guardrails:** Process spawning strictly uses argument arrays (`Command::new(...).args(...)`) rather than shell interpolation (`sh -c`). Pre- and post-launch commands from imported third-party packs (MultiMC/Prism/Modrinth/CurseForge) are disabled by default with prominent warnings.
4. **Archive Safety:** Strict Zip-Slip and path traversal verification prevents files from writing outside target instance roots. Archive extraction enforces limits on decompression ratio and maximum total size to prevent zip-bomb attacks.
