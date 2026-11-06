//! Ely.by Yggdrasil & OAuth2 authentication provider and `authlib-injector.jar` manager.
//!
//! Implements:
//! - Yggdrasil `/auth/authenticate` (including 2FA TOTP support via `password:totp`),
//!   `/auth/refresh`, and `/auth/validate`.
//! - Browser OAuth2 authorization URL construction.
//! - Automatic download, SHA-256 verification, and atomic caching of `authlib-injector.jar`
//!   into `<data_dir>/cache/authlib-injector.jar`.
//! - JVM `-javaagent` argument construction for Ely.by skin and session injection.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use mq_core::{AccountId, MqError, MqPaths, MqResult, SecretString};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{AccountKind, AccountProfile, AuthlibInjectorInfo};
use crate::offline::compute_offline_uuid;
use crate::skin::generate_offline_geometric_avatar;

/// Default Ely.by Yggdrasil authentication server base URL.
pub const ELYBY_AUTH_BASE_URL: &str = "https://authserver.ely.by";

/// Default Ely.by `authlib-injector` configuration metadata URL passed to `-javaagent`.
pub const ELYBY_INJECTOR_CONFIG_URL: &str = "https://account.ely.by/api/authlib-injector";

/// Default Ely.by skin server base URL (`{base}/{username}.png`).
pub const ELYBY_SKINS_BASE_URL: &str = "http://skinsystem.ely.by/skins";

/// Default upstream artifact descriptor URL for the latest `authlib-injector.jar` release.
pub const AUTHLIB_INJECTOR_LATEST_URL: &str =
    "https://authlib-injector.yushi.moe/artifact/latest.json";

/// Default Ely.by OAuth2 browser authorization endpoint.
pub const ELYBY_OAUTH2_AUTHORIZE_URL: &str = "https://account.ely.by/oauth2/v1";

/// Client for Ely.by Yggdrasil authentication, skin lookup, and `authlib-injector` management.
#[derive(Debug, Clone)]
pub struct ElyByClient {
    http: reqwest::Client,
    auth_base_url: String,
    injector_config_url: String,
    skins_base_url: String,
    authlib_injector_latest_url: String,
}

impl Default for ElyByClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ElyByClient {
    /// Creates a new [`ElyByClient`] targeting production Ely.by endpoints.
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("mq-launcher/0.1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self::with_http_client(http)
    }

    /// Creates a new [`ElyByClient`] using the supplied [`reqwest::Client`].
    pub fn with_http_client(http: reqwest::Client) -> Self {
        Self {
            http,
            auth_base_url: ELYBY_AUTH_BASE_URL.to_owned(),
            injector_config_url: ELYBY_INJECTOR_CONFIG_URL.to_owned(),
            skins_base_url: ELYBY_SKINS_BASE_URL.to_owned(),
            authlib_injector_latest_url: AUTHLIB_INJECTOR_LATEST_URL.to_owned(),
        }
    }

    /// Creates an [`ElyByClient`] pre-configured against a local mock server root URL
    /// (matching the route layout used by `testsupport::MockServer`).
    pub fn with_mock_base_url(base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/');
        Self::new()
            .with_auth_base_url(format!("{base}/ely"))
            .with_injector_config_url(format!("{base}/ely/authlib-injector"))
            .with_skins_base_url(format!("{base}/ely/skins"))
            .with_authlib_injector_latest_url(format!(
                "{base}/authlib-injector/artifact/latest.json"
            ))
    }

    /// Overrides the Yggdrasil authentication base URL (default: [`ELYBY_AUTH_BASE_URL`]).
    pub fn with_auth_base_url(mut self, auth_base_url: impl Into<String>) -> Self {
        self.auth_base_url = auth_base_url.into().trim_end_matches('/').to_owned();
        self
    }

    /// Overrides the `authlib-injector` configuration URL (default: [`ELYBY_INJECTOR_CONFIG_URL`]).
    pub fn with_injector_config_url(mut self, injector_config_url: impl Into<String>) -> Self {
        self.injector_config_url = injector_config_url.into();
        self
    }

    /// Overrides the skins server base URL (default: [`ELYBY_SKINS_BASE_URL`]).
    pub fn with_skins_base_url(mut self, skins_base_url: impl Into<String>) -> Self {
        self.skins_base_url = skins_base_url.into().trim_end_matches('/').to_owned();
        self
    }

    /// Overrides the `authlib-injector` `latest.json` metadata URL (default: [`AUTHLIB_INJECTOR_LATEST_URL`]).
    pub fn with_authlib_injector_latest_url(mut self, latest_url: impl Into<String>) -> Self {
        self.authlib_injector_latest_url = latest_url.into();
        self
    }

    /// Alias for [`Self::with_authlib_injector_latest_url`].
    pub fn with_latest_json_url(self, latest_url: impl Into<String>) -> Self {
        self.with_authlib_injector_latest_url(latest_url)
    }

    /// Borrows the underlying HTTP client.
    #[inline]
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http
    }

    /// Returns the configured Yggdrasil authentication base URL.
    #[inline]
    pub fn auth_base_url(&self) -> &str {
        &self.auth_base_url
    }

    /// Returns the configured `authlib-injector` metadata URL.
    #[inline]
    pub fn injector_config_url(&self) -> &str {
        &self.injector_config_url
    }

    /// Returns the configured skins base URL.
    #[inline]
    pub fn skins_base_url(&self) -> &str {
        &self.skins_base_url
    }

    /// Returns the configured `authlib-injector` `latest.json` URL.
    #[inline]
    pub fn authlib_injector_latest_url(&self) -> &str {
        &self.authlib_injector_latest_url
    }

    /// Authenticates an Ely.by account via `POST {auth_base}/auth/authenticate`.
    ///
    /// If `totp_token` is provided and non-empty, appends `:totp` to the password per
    /// Ely.by's Yggdrasil 2FA specification (and includes the `totp` field for mock/extended
    /// servers). Never logs credentials or session tokens.
    pub async fn authenticate(
        &self,
        login: &str,
        password: &str,
        totp_token: Option<&str>,
        client_token: Option<&str>,
    ) -> MqResult<AccountProfile> {
        let login = login.trim();
        if login.is_empty() || password.is_empty() {
            return Err(MqError::Auth(
                "Ely.by login and password must not be empty".to_owned(),
            ));
        }

        let clean_totp = totp_token.map(str::trim).filter(|t| !t.is_empty());
        let combined_password = match clean_totp {
            Some(totp) if !password.ends_with(&format!(":{totp}")) => {
                SecretString::new(format!("{password}:{totp}"))
            }
            _ => SecretString::new(password),
        };

        let clean_client_token = client_token
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);

        let url = format!("{}/auth/authenticate", self.auth_base_url);
        let payload = YggdrasilAuthRequest {
            agent: YggdrasilAgent::minecraft(),
            username: login,
            password: combined_password.expose_secret(),
            client_token: clean_client_token.as_deref(),
            request_user: true,
            totp: clean_totp,
        };

        let resp = self
            .http
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| {
                MqError::Network(format!("Ely.by authenticate request failed: {err}"))
            })?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(map_yggdrasil_http_error(status, &url, &body_text));
        }

        let parsed: YggdrasilAuthResponse = serde_json::from_str(&body_text)?;
        self.build_profile_from_response(parsed, clean_client_token, None)
    }

    /// Refreshes an existing Ely.by session via `POST {auth_base}/auth/refresh`.
    pub async fn refresh(&self, profile: &AccountProfile) -> MqResult<AccountProfile> {
        let url = format!("{}/auth/refresh", self.auth_base_url);
        let client_tok = profile
            .client_token
            .as_ref()
            .map(SecretString::expose_secret);

        let payload = YggdrasilRefreshRequest {
            access_token: profile.access_token.expose_secret(),
            client_token: client_tok,
            request_user: true,
        };

        let resp = self
            .http
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| MqError::Network(format!("Ely.by refresh request failed: {err}")))?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(map_yggdrasil_http_error(status, &url, &body_text));
        }

        let parsed: YggdrasilAuthResponse = serde_json::from_str(&body_text)?;
        let fallback_ct = client_tok.map(str::to_owned);
        self.build_profile_from_response(parsed, fallback_ct, Some(profile))
    }

    /// Validates an Ely.by access token via `POST {auth_base}/auth/validate`.
    ///
    /// Returns `Ok(true)` on HTTP `200` or `204`, and `Ok(false)` on HTTP `401` or `403`.
    pub async fn validate(&self, profile: &AccountProfile) -> MqResult<bool> {
        let url = format!("{}/auth/validate", self.auth_base_url);
        let client_tok = profile
            .client_token
            .as_ref()
            .map(SecretString::expose_secret);

        let payload = YggdrasilValidateRequest {
            access_token: profile.access_token.expose_secret(),
            client_token: client_tok,
        };

        let resp = self
            .http
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| MqError::Network(format!("Ely.by validate request failed: {err}")))?;

        match resp.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(true),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Ok(false),
            other => {
                let body = resp.text().await.unwrap_or_default();
                Err(MqError::HttpStatus {
                    status: other.as_u16(),
                    url,
                    body,
                })
            }
        }
    }

    /// Builds a browser OAuth2 authorization URL for Ely.by login.
    #[inline]
    pub fn oauth2_browser_authorize_url(
        client_id: &str,
        redirect_uri: &str,
        state: &str,
    ) -> String {
        oauth2_browser_authorize_url(client_id, redirect_uri, state)
    }

    /// Fetches the latest `authlib-injector.jar` release metadata from the configured `latest.json` URL.
    pub async fn fetch_authlib_injector_info(&self) -> MqResult<AuthlibInjectorInfo> {
        let url = self.authlib_injector_latest_url.clone();
        let resp = self.http.get(&url).send().await.map_err(|err| {
            MqError::Network(format!(
                "failed to fetch authlib-injector metadata from {url}: {err}"
            ))
        })?;

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url,
                body: body_text,
            });
        }

        let raw: RawAuthlibLatestJson = serde_json::from_str(&body_text)?;
        let sha256 = raw
            .checksums
            .and_then(|c| c.sha256)
            .or(raw.sha256)
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                MqError::Metadata(
                    "authlib-injector latest.json is missing checksums.sha256".to_owned(),
                )
            })?;

        Ok(AuthlibInjectorInfo {
            build_number: raw.build_number.unwrap_or(0),
            version: raw.version.unwrap_or_else(|| "unknown".to_owned()),
            download_url: raw.download_url,
            sha256,
        })
    }

    /// Ensures `<data_dir>/cache/authlib-injector.jar` is present and verified against SHA-256.
    ///
    /// - If the JAR already exists on disk, is non-empty, and matches its `.sha256` sidecar
    ///   (when present), returns the cached path immediately without network I/O.
    /// - Otherwise fetches `latest.json`, downloads the artifact, verifies its `SHA-256` digest
    ///   (returning [`MqError::HashMismatch`] on mismatch), atomically writes the JAR to
    ///   `paths.authlib_injector_jar()`, writes the `.sha256` sidecar, and returns the path.
    pub async fn ensure_authlib_injector(&self, paths: &MqPaths) -> MqResult<PathBuf> {
        let jar_path = paths.authlib_injector_jar();
        let sidecar_primary = sha256_sidecar_path(&jar_path);
        let sidecar_alt = jar_path.with_extension("sha256");

        if let Ok(existing_bytes) = fs::read(&jar_path) {
            if !existing_bytes.is_empty() {
                let recorded_sha = fs::read_to_string(&sidecar_primary)
                    .or_else(|_| fs::read_to_string(&sidecar_alt))
                    .ok()
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty());

                match recorded_sha {
                    Some(expected_hex) => {
                        let actual_hex = compute_sha256_hex(&existing_bytes);
                        if actual_hex == expected_hex {
                            return Ok(jar_path);
                        }
                    }
                    None => {
                        return Ok(jar_path);
                    }
                }
            }
        }

        let info = self.fetch_authlib_injector_info().await?;
        let dl_resp = self
            .http
            .get(&info.download_url)
            .send()
            .await
            .map_err(|err| {
                MqError::Network(format!(
                    "failed to download authlib-injector.jar from {}: {err}",
                    info.download_url
                ))
            })?;

        let status = dl_resp.status();
        if !status.is_success() {
            let body = dl_resp.text().await.unwrap_or_default();
            return Err(MqError::HttpStatus {
                status: status.as_u16(),
                url: info.download_url,
                body,
            });
        }

        let jar_bytes = dl_resp.bytes().await.map_err(|err| {
            MqError::Network(format!(
                "failed reading authlib-injector.jar body from {}: {err}",
                info.download_url
            ))
        })?;

        let actual_sha256 = compute_sha256_hex(&jar_bytes);
        let expected_sha256 = info.sha256.trim().to_ascii_lowercase();
        if actual_sha256 != expected_sha256 {
            return Err(MqError::HashMismatch {
                target: "authlib-injector.jar".to_owned(),
                algo: "SHA-256",
                expected: expected_sha256,
                actual: actual_sha256,
            });
        }

        if let Some(parent) = jar_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_name = format!(".authlib-injector.jar.part.{}", Uuid::new_v4().simple());
        let tmp_path = jar_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(tmp_name);

        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(&jar_bytes)?;
            file.sync_all()?;
        }

        fs::rename(&tmp_path, &jar_path)?;
        let _ = fs::write(&sidecar_primary, format!("{expected_sha256}\n"));
        let _ = fs::write(&sidecar_alt, format!("{expected_sha256}\n"));

        Ok(jar_path)
    }

    /// Returns the JVM arguments required to inject `authlib-injector.jar` using this client's
    /// configured injector metadata URL.
    pub fn jvm_injection_args(&self, injector_jar_path: &Path) -> [String; 2] {
        [
            format!(
                "-javaagent:{}={}",
                injector_jar_path.display(),
                self.injector_config_url
            ),
            "-Dauthlibinjector.side=client".to_owned(),
        ]
    }

    fn build_profile_from_response(
        &self,
        resp: YggdrasilAuthResponse,
        fallback_client_token: Option<String>,
        existing: Option<&AccountProfile>,
    ) -> MqResult<AccountProfile> {
        let access_token = resp.access_token.trim().to_owned();
        if access_token.is_empty() {
            return Err(MqError::Auth(
                "Ely.by response contained an empty accessToken".to_owned(),
            ));
        }

        let (username, uuid) = if let Some(sel) = resp.selected_profile {
            let parsed_uuid =
                Uuid::parse_str(sel.id.trim()).unwrap_or_else(|_| compute_offline_uuid(&sel.name));
            (sel.name, parsed_uuid)
        } else if let Some(prev) = existing {
            (prev.username.clone(), prev.uuid)
        } else {
            return Err(MqError::Auth(
                "Ely.by response is missing selectedProfile".to_owned(),
            ));
        };

        let id = existing
            .map(|p| p.id)
            .unwrap_or_else(|| AccountId::from_uuid(uuid));

        let client_token = resp
            .client_token
            .filter(|s| !s.trim().is_empty())
            .or(fallback_client_token)
            .map(SecretString::new)
            .or_else(|| existing.and_then(|p| p.client_token.clone()));

        let skin_url = existing
            .and_then(|p| p.skin_url.clone())
            .or_else(|| Some(format!("{}/{username}.png", self.skins_base_url)));

        let avatar_head = existing
            .and_then(|p| p.avatar_head.clone())
            .or_else(|| Some(generate_offline_geometric_avatar(&username)));

        Ok(AccountProfile {
            id,
            kind: AccountKind::ElyBy,
            username,
            uuid,
            access_token: SecretString::new(access_token),
            refresh_token: None,
            client_token,
            expires_at: None,
            skin_url,
            avatar_head,
        })
    }
}

/// Builds the two JVM arguments required to launch Minecraft with Ely.by's `authlib-injector`:
/// `["-javaagent:<jar_path>=https://account.ely.by/api/authlib-injector", "-Dauthlibinjector.side=client"]`.
pub fn elyby_jvm_injection_args(injector_jar_path: &Path) -> [String; 2] {
    [
        format!(
            "-javaagent:{}={}",
            injector_jar_path.display(),
            ELYBY_INJECTOR_CONFIG_URL
        ),
        "-Dauthlibinjector.side=client".to_owned(),
    ]
}

/// Builds an OAuth2 authorization URL for browser-based Ely.by sign-in.
pub fn oauth2_browser_authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=account_info%20minecraft_server_session&state={}",
        ELYBY_OAUTH2_AUTHORIZE_URL,
        percent_encode_query_component(client_id),
        percent_encode_query_component(redirect_uri),
        percent_encode_query_component(state),
    )
}

fn percent_encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn sha256_sidecar_path(jar_path: &Path) -> PathBuf {
    let mut os = jar_path.as_os_str().to_os_string();
    os.push(".sha256");
    PathBuf::from(os)
}

fn compute_sha256_hex(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    hex::encode(digest)
}

fn map_yggdrasil_http_error(status: StatusCode, url: &str, body_text: &str) -> MqError {
    if status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
    {
        return MqError::HttpStatus {
            status: status.as_u16(),
            url: url.to_owned(),
            body: body_text.to_owned(),
        };
    }

    if let Ok(err_json) = serde_json::from_str::<YggdrasilErrorPayload>(body_text) {
        let mut msg = format!(
            "Ely.by authentication failed (HTTP {}): {}",
            status.as_u16(),
            err_json
                .error_message
                .or(err_json.error)
                .unwrap_or_else(|| "unknown error".to_owned())
        );
        if let Some(cause) = err_json.cause.filter(|c| !c.is_empty()) {
            msg.push_str(&format!(" ({cause})"));
        }
        return MqError::Auth(msg);
    }

    MqError::Auth(format!(
        "Ely.by authentication failed (HTTP {}): {}",
        status.as_u16(),
        body_text
    ))
}

#[derive(Debug, Serialize)]
struct YggdrasilAgent {
    name: &'static str,
    version: u8,
}

impl YggdrasilAgent {
    const fn minecraft() -> Self {
        Self {
            name: "Minecraft",
            version: 1,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilAuthRequest<'a> {
    agent: YggdrasilAgent,
    username: &'a str,
    password: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_token: Option<&'a str>,
    request_user: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    totp: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilRefreshRequest<'a> {
    access_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_token: Option<&'a str>,
    request_user: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilValidateRequest<'a> {
    access_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_token: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilAuthResponse {
    access_token: String,
    #[serde(default)]
    client_token: Option<String>,
    #[serde(default)]
    selected_profile: Option<YggdrasilProfile>,
}

#[derive(Debug, Deserialize)]
struct YggdrasilProfile {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YggdrasilErrorPayload {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    cause: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAuthlibLatestJson {
    #[serde(default)]
    build_number: Option<u32>,
    #[serde(default)]
    version: Option<String>,
    download_url: String,
    #[serde(default)]
    checksums: Option<RawAuthlibChecksums>,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAuthlibChecksums {
    #[serde(default)]
    sha256: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use serde_json::{json, Value};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    async fn read_http_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let mut buf = Vec::with_capacity(2048);
        let mut temp = [0u8; 1024];
        let header_end = loop {
            let n = stream.read(&mut temp).await.expect("read request chunk");
            if n == 0 {
                break buf.len();
            }
            buf.extend_from_slice(&temp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos;
            }
        };

        let header_str = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let content_len = header_str
            .lines()
            .find_map(|line| {
                let (k, v) = line.split_once(':')?;
                if k.trim().eq_ignore_ascii_case("content-length") {
                    v.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let body_start = (header_end + 4).min(buf.len());
        while buf.len() - body_start < content_len {
            let n = stream.read(&mut temp).await.expect("read body chunk");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&temp[..n]);
        }

        let request_line = header_str.lines().next().unwrap_or_default().to_string();
        let body = buf[body_start..].to_vec();
        (request_line, body)
    }

    async fn write_http_response(
        stream: &mut TcpStream,
        status_line: &str,
        content_type: &str,
        body: &[u8],
    ) {
        let headers = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    }

    #[test]
    fn elyby_jvm_injection_args_match_expected_format() {
        let jar_path = Path::new("/home/player/.local/share/mq/cache/authlib-injector.jar");
        let args = elyby_jvm_injection_args(jar_path);
        assert_eq!(
            args,
            [
                "-javaagent:/home/player/.local/share/mq/cache/authlib-injector.jar=https://account.ely.by/api/authlib-injector".to_string(),
                "-Dauthlibinjector.side=client".to_string(),
            ]
        );
    }

    #[test]
    fn oauth2_browser_authorize_url_encodes_parameters() {
        let url = oauth2_browser_authorize_url(
            "mq-launcher",
            "http://127.0.0.1:28491/callback",
            "csrf_state_123",
        );
        assert!(url.starts_with(ELYBY_OAUTH2_AUTHORIZE_URL));
        assert!(url.contains("client_id=mq-launcher"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A28491%2Fcallback"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=csrf_state_123"));
    }

    #[tokio::test]
    async fn yggdrasil_authenticate_with_2fa_validate_and_refresh_against_local_tcp_server() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local TCP server");
        let addr = listener.local_addr().expect("local addr");

        let server_task = tokio::spawn(async move {
            // 1. POST /auth/authenticate (verify agent, username, password:totp, clientToken)
            {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (req_line, body) = read_http_request(&mut stream).await;
                assert_eq!(req_line, "POST /auth/authenticate HTTP/1.1");
                let json_body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(json_body["agent"]["name"], "Minecraft");
                assert_eq!(json_body["agent"]["version"], 1);
                assert_eq!(json_body["username"], "ElyPlayer2FA");
                assert_eq!(json_body["password"], "secretPass:654321");
                assert_eq!(json_body["clientToken"], "ct-001");
                assert_eq!(json_body["requestUser"], true);

                let resp = serde_json::to_vec(&json!({
                    "accessToken": "ely_access_token_v1",
                    "clientToken": "ct-001",
                    "selectedProfile": {
                        "id": "b50ad385829d3141a2167e7d7539ba7f",
                        "name": "ElyPlayer2FA"
                    }
                }))
                .unwrap();
                write_http_response(&mut stream, "200 OK", "application/json", &resp).await;
            }

            // 2. POST /auth/validate -> 204 No Content
            {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (req_line, body) = read_http_request(&mut stream).await;
                assert_eq!(req_line, "POST /auth/validate HTTP/1.1");
                let json_body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(json_body["accessToken"], "ely_access_token_v1");
                assert_eq!(json_body["clientToken"], "ct-001");
                write_http_response(&mut stream, "204 No Content", "text/plain", b"").await;
            }

            // 3. POST /auth/refresh -> 200 OK with rotated accessToken
            {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (req_line, body) = read_http_request(&mut stream).await;
                assert_eq!(req_line, "POST /auth/refresh HTTP/1.1");
                let json_body: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(json_body["accessToken"], "ely_access_token_v1");
                assert_eq!(json_body["clientToken"], "ct-001");

                let resp = serde_json::to_vec(&json!({
                    "accessToken": "ely_access_token_v2",
                    "clientToken": "ct-001",
                    "selectedProfile": {
                        "id": "b50ad385829d3141a2167e7d7539ba7f",
                        "name": "ElyPlayer2FA"
                    }
                }))
                .unwrap();
                write_http_response(&mut stream, "200 OK", "application/json", &resp).await;
            }

            // 4. POST /auth/validate with invalid token -> 403 Forbidden
            {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (req_line, _) = read_http_request(&mut stream).await;
                assert_eq!(req_line, "POST /auth/validate HTTP/1.1");
                let resp = serde_json::to_vec(&json!({
                    "error": "ForbiddenOperationException",
                    "errorMessage": "Invalid token."
                }))
                .unwrap();
                write_http_response(&mut stream, "403 Forbidden", "application/json", &resp).await;
            }
        });

        let client = ElyByClient::new().with_auth_base_url(format!("http://{addr}"));
        let profile = client
            .authenticate("ElyPlayer2FA", "secretPass", Some("654321"), Some("ct-001"))
            .await
            .expect("authenticate with 2FA");

        assert_eq!(profile.kind, AccountKind::ElyBy);
        assert_eq!(profile.username, "ElyPlayer2FA");
        assert_eq!(profile.uuid_simple(), "b50ad385829d3141a2167e7d7539ba7f");
        assert_eq!(profile.access_token.expose_secret(), "ely_access_token_v1");
        assert_eq!(
            profile
                .client_token
                .as_ref()
                .map(SecretString::expose_secret),
            Some("ct-001")
        );
        // SecretString debug output must never leak the token.
        assert!(!format!("{profile:?}").contains("ely_access_token_v1"));

        let is_valid = client.validate(&profile).await.expect("validate token");
        assert!(is_valid);

        let refreshed = client.refresh(&profile).await.expect("refresh token");
        assert_eq!(
            refreshed.access_token.expose_secret(),
            "ely_access_token_v2"
        );
        assert_eq!(refreshed.id, profile.id);

        let old_valid = client
            .validate(&profile)
            .await
            .expect("validate expired token");
        assert!(!old_valid);

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn ensure_authlib_injector_downloads_verifies_sha256_caches_and_rejects_mismatch() {
        let fake_jar_bytes = b"PK\x03\x04-synthetic-authlib-injector-agent-payload".to_vec();
        let expected_sha256 = compute_sha256_hex(&fake_jar_bytes);

        // 1. Test successful download + SHA-256 verification + cache hit without extra requests.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = Arc::clone(&hits);
        let jar_for_server = fake_jar_bytes.clone();
        let sha_for_server = expected_sha256.clone();

        let server_task = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                hits_clone.fetch_add(1, Ordering::SeqCst);
                let (req_line, _) = read_http_request(&mut stream).await;
                if req_line.starts_with("GET /artifact/latest.json ") {
                    let meta = serde_json::to_vec(&json!({
                        "build_number": 53,
                        "version": "1.2.5",
                        "download_url": format!("http://{addr}/download/authlib-injector.jar"),
                        "checksums": {
                            "sha256": sha_for_server
                        }
                    }))
                    .unwrap();
                    write_http_response(&mut stream, "200 OK", "application/json", &meta).await;
                } else if req_line.starts_with("GET /download/authlib-injector.jar ") {
                    write_http_response(
                        &mut stream,
                        "200 OK",
                        "application/java-archive",
                        &jar_for_server,
                    )
                    .await;
                }
            }
        });

        let tmp = tempfile::tempdir().expect("create tempdir");
        let paths = MqPaths::with_root(tmp.path());
        let client = ElyByClient::new()
            .with_authlib_injector_latest_url(format!("http://{addr}/artifact/latest.json"));

        let jar_path = client
            .ensure_authlib_injector(&paths)
            .await
            .expect("download and verify authlib-injector.jar");
        assert_eq!(jar_path, paths.authlib_injector_jar());
        assert_eq!(fs::read(&jar_path).unwrap(), fake_jar_bytes);
        assert_eq!(hits.load(Ordering::SeqCst), 2);

        server_task.await.unwrap();

        // Second call must hit the verified local cache with zero network requests.
        let cached_path = client
            .ensure_authlib_injector(&paths)
            .await
            .expect("reuse cached authlib-injector.jar");
        assert_eq!(cached_path, jar_path);

        // 2. Test SHA-256 mismatch rejection (`MqError::HashMismatch`).
        let bad_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bad_addr = bad_listener.local_addr().unwrap();

        let bad_server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = bad_listener.accept().await.unwrap();
                let (req_line, _) = read_http_request(&mut stream).await;
                if req_line.starts_with("GET /artifact/latest.json ") {
                    let meta = serde_json::to_vec(&json!({
                        "build_number": 53,
                        "version": "1.2.5",
                        "download_url": format!("http://{bad_addr}/download/authlib-injector.jar"),
                        "checksums": {
                            "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                        }
                    }))
                    .unwrap();
                    write_http_response(&mut stream, "200 OK", "application/json", &meta).await;
                } else {
                    write_http_response(
                        &mut stream,
                        "200 OK",
                        "application/java-archive",
                        b"tampered-jar-bytes",
                    )
                    .await;
                }
            }
        });

        let tmp_bad = tempfile::tempdir().expect("create tempdir for mismatch test");
        let bad_paths = MqPaths::with_root(tmp_bad.path());
        let bad_client = ElyByClient::new()
            .with_authlib_injector_latest_url(format!("http://{bad_addr}/artifact/latest.json"));

        let err = bad_client
            .ensure_authlib_injector(&bad_paths)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                MqError::HashMismatch {
                    algo: "SHA-256",
                    ..
                }
            ),
            "expected SHA-256 HashMismatch, got {err:?}"
        );
        assert!(
            !bad_paths.authlib_injector_jar().exists(),
            "corrupted JAR must never be committed to final path"
        );

        bad_server.await.unwrap();
    }
}
