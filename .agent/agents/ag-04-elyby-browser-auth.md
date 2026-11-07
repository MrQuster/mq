# Agent 04: Ely.by Browser OAuth2 & Account Management

- **Zone:** `crates/mq-auth/`, `crates/mq-ui/src/accounts_view.rs`
- **Ownership:** Ely.by OAuth2 PKCE engine, browser flow, ephemeral loopback server on `127.0.0.1:<port>/callback`, token storage on Linux (`keyring` / 0600) and Windows (Credential Manager / DPAPI / encrypted store), UI accounts dialog, `mq accounts add elyby`.
- **Goals:**
  1. Default Client ID: `"mq-launcher"` (no client secret; public client PKCE).
  2. Implement RFC 7636 PKCE: generate high-entropy `code_verifier`, compute `code_challenge = Base64URL(SHA256(code_verifier))`.
  3. Authorization URL: `https://account.ely.by/oauth2/v1?client_id=mq-launcher&redirect_uri=http://127.0.0.1:<port>/callback&response_type=code&scope=account_info%20account_email%20minecraft_server_session&state=<state>&code_challenge=<challenge>&code_challenge_method=S256`.
  4. Ephemeral loopback listener on `127.0.0.1:<port>` with 5-minute timeout and localized Russian completion page.
  5. Exchange code for access token without client secret.
  6. Store tokens safely: Linux Secret Service (`keyring`), Windows Credential Manager (`keyring` with DPAPI fallback).
  7. UI: "Ely.by (через браузер)", "Microsoft (через браузер)", "Офлайн (только ник)". No password/email inputs.
  8. Delete legacy password auth code and endpoints.
  9. Mock server unit tests for full OAuth2 lifecycle, PKCE, CSRF protection, and token refresh.
