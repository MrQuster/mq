---
name: mq-elyby-auth
description: Official Ely.by OAuth2 browser authentication, loopback listener, and authlib-injector
---

# Ely.by OAuth2 Browser-Only Authentication

## Endpoint Reference
- Authorize: `https://account.ely.by/oauth2/v1?client_id={id}&redirect_uri={uri}&response_type=code&scope=account_info%20account_email%20offline_access%20minecraft_server_session&state={state}`
- Token Exchange: `POST https://account.ely.by/api/oauth2/v1/token`
  - Body: `grant_type=authorization_code&client_id={id}&client_secret={secret}&code={code}&redirect_uri={uri}`
- User Info: `GET https://account.ely.by/api/account/v1/info` (`Authorization: Bearer <access_token>`)
  - Returns: `{ "id": 1, "uuid": "...", "username": "..." }`
- Token Refresh: `POST https://account.ely.by/api/oauth2/v1/token`
  - Body: `grant_type=refresh_token&client_id={id}&client_secret={secret}&refresh_token={token}`

## Architecture & Security
- Loopback TCP Server on `127.0.0.1:<port>` (e.g. port 25585 or ephemeral).
- CSRF protection: validate `state` parameter matches.
- Returns HTML response: "Вход выполнен. Можно вернуться в лаунчер".
- Token storage: Linux Secret Service (`keyring`), fallback file `0600`.
- Zero credentials in UI: no password / email / username text inputs.
- Minecraft Launch Argument:
  `-javaagent:<path_to_authlib_injector>.jar=https://authserver.ely.by`
  `--username <username>`
  `--uuid <uuid>`
  `--accessToken <access_token>`
