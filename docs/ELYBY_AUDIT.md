# Ely.by OAuth2 Integration Audit & Specification (v2.2.0)

Этот документ фиксирует анализ официальной документации Ely.by (https://docs.ely.by) и требования к реализации авторизации в MQ Launcher.

---

## 1. Официальные спецификации Ely.by

1. **Тип клиента:**
   - Настольное приложение («Настольное приложение» / Public Client).
   - `client_id = "mq-launcher"` (константа по умолчанию с переопределением через `MQ_ELYBY_CLIENT_ID` и `config.toml` `[elyby]`).
   - Секрета нет (`client_secret` отсутствует для публичных клиентов).

2. **Протокол:**
   - Authorization Code Flow с PKCE (RFC 7636).
   - `code_challenge_method = "S256"`.
   - `code_verifier`: 43–128 символов случайной энтропии.
   - `code_challenge`: `Base64URL(SHA256(code_verifier))`.

3. **Точки входа (Endpoints):**
   - URL авторизации: `https://account.ely.by/oauth2/v1`
   - URL обмена токена: `POST https://account.ely.by/api/oauth2/v1/token`
   - URL профиля пользователя: `GET https://account.ely.by/api/account/v1/info`
   - URL скинов: `https://skinsystem.ely.by/skins/{username}.png`

4. **Параметры авторизации:**
   - `client_id`: `mq-launcher`
   - `redirect_uri`: `http://127.0.0.1:<port>/callback` (одноразовый локальный слушатель)
   - `response_type`: `code`
   - `scope`: `account_info account_email minecraft_server_session`
   - `state`: случайная криптографическая строка против CSRF.
   - `code_challenge`: вычисленный PKCE challenge.
   - `code_challenge_method`: `S256`.

5. **Использование в Minecraft (`authlib-injector`):**
   - Java-агент подключается в начало аргументов JVM:
     `-javaagent:<path>/authlib-injector.jar=ely.by`
   - Передаваемые аргументы запуска игры:
     `--username <username> --uuid <uuid> --accessToken <access_token>`
   - `authlib-injector` верифицирует `access_token` на сервере сессий Ely.by (`https://authserver.ely.by`).

---

## 2. Изменения интерфейса и безопасности

- В GUI и CLI полностью удалены любые поля ввода логина/пароля.
- Добавлена вторичная кнопка "Создать аккаунт на Ely.by" (`https://account.ely.by/register`).
- Состояние ожидания: "Ожидание подтверждения входа в браузере..." с кнопками "Открыть ссылку ещё раз", "Скопировать ссылку", "Отмена".
- Таймаут ожидания: 5 минут.
- Страница завершения в браузере: локализованный HTML "Успешный вход в MQ Launcher. Вы можете закрыть эту вкладку." без внешних ресурсов.
