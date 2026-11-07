---
name: mq-elyby-auth
description: Ely.by OAuth2 PKCE browser-only flow and authlib-injector specification
---

# Ely.by Browser OAuth2 & authlib-injector Specification

## 1. Application Registration & Credentials
- Application Type: "Настольное приложение" (Desktop public client).
- Default Client ID: `"mq-launcher"` (configurable via `MQ_ELYBY_CLIENT_ID` or `config.toml` `[elyby]`).
- Public client: NO `client_secret`.
- Flow: Authorization Code Flow with PKCE (S256).

## 2. Authorization URL Generation
- Base URL: `https://account.ely.by/oauth2/v1`
- Query Parameters:
  - `client_id`: `mq-launcher`
  - `redirect_uri`: `http://127.0.0.1:<port>/callback` (ephemeral or fixed 25585 loopback)
  - `response_type`: `code`
  - `scope`: `account_info account_email minecraft_server_session`
  - `state`: 32-byte cryptographically random hex/UUID string
  - `code_challenge`: Base64URL(SHA256(code_verifier))
  - `code_challenge_method`: `S256`

## 3. Loopback Callback & Token Exchange
- Ephemeral listener on `127.0.0.1:<port>` with timeout (5 minutes).
- Return localized Russian HTML page in user browser: "Успешный вход в MQ Launcher. Вы можете закрыть эту вкладку."
- Exchange code at `POST https://account.ely.by/api/oauth2/v1/token`:
  - `grant_type`: `authorization_code`
  - `client_id`: `mq-launcher`
  - `code`: captured code
  - `redirect_uri`: exact matching loopback URL
  - `code_verifier`: original PKCE verifier string
- Response: `access_token`, `refresh_token`, `expires_in`.
- User Profile fetch: `GET https://account.ely.by/api/account/v1/info` with `Bearer <access_token>`.

## 4. Minecraft Launch Integration
- Download verified `authlib-injector.jar` with SHA-256 verification from official repository.
- Pass Java agent first in JVM args:
  `-javaagent:<path>/authlib-injector.jar=ely.by` (or `-javaagent:<path>/authlib-injector.jar=https://authserver.ely.by/api/yggdrasil`)
- Pass `--username <username> --uuid <uuid> --accessToken <access_token>` to Minecraft client.
