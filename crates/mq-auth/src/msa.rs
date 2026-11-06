//! Microsoft Account (MSA) authentication: OAuth2 Device Code Flow -> Xbox Live (`XBL`)
//! -> Xbox Secure Token Service (`XSTS`) -> Minecraft Services (`login_with_xbox`,
//! `/entitlements/mcstore`, and `/minecraft/profile`).
//!
//! Security invariants:
//! - Access tokens, refresh tokens, Xbox/XSTS tokens, and device codes are wrapped in
//!   [`SecretString`] and never written to logs or exposed in `Debug`/`Display`.
//! - Any error payload returned by remote endpoints is scrubbed via [`redact_text`]
//!   before surfacing in [`MqError`].

use std::time::{Duration, Instant};

use chrono::Utc;
use mq_core::{
    redact_text, AccountId, AppConfig, MqError, MqResult, SecretString, ENV_MS_CLIENT_ID,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::models::{AccountKind, AccountProfile};

/// Default OAuth2 scopes requested for Xbox Live sign-in and refresh token issuance.
pub const MSA_OAUTH_SCOPE: &str = "XboxLive.signin offline_access";

/// Actionable user-facing guidance when no Microsoft OAuth2 Client ID is configured.
pub const MS_CLIENT_ID_SETUP_HINT: &str = "Microsoft login requires an OAuth2 Client ID. \
Set the MQ_MS_CLIENT_ID environment variable or configure `ms_client_id` in Settings / config.toml \
(Azure App Registration -> Personal Microsoft accounts -> Allow public client / device code flow). \
Offline and Ely.by accounts work out of the box without any keys.";

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_USER_AGENT: &str = concat!("mq-launcher/", env!("CARGO_PKG_VERSION"));

/// Returns actionable instructions explaining how to configure `MQ_MS_CLIENT_ID` while
/// noting that Offline and Ely.by authentication work out of the box without keys.
#[inline]
pub const fn ms_client_id_setup_hint() -> &'static str {
    MS_CLIENT_ID_SETUP_HINT
}

/// Resolves the Microsoft OAuth2 Client ID from `$MQ_MS_CLIENT_ID` or [`AppConfig::ms_client_id`].
///
/// If unset or blank, logs actionable setup guidance (noting that Offline and Ely.by
/// work without any keys) and returns [`MqError::MissingClientId`].
pub fn resolve_ms_client_id(config: &AppConfig) -> MqResult<String> {
    match config.effective_ms_client_id() {
        Some(client_id) => Ok(client_id),
        None => {
            debug!("{MS_CLIENT_ID_SETUP_HINT}");
            Err(MqError::MissingClientId {
                env_var: ENV_MS_CLIENT_ID,
            })
        }
    }
}

/// Deterministic helper resolving the Microsoft OAuth2 Client ID with an explicit
/// environment override instead of reading process-global environment variables.
pub fn resolve_ms_client_id_with_env(
    config: &AppConfig,
    env_override: Option<&str>,
) -> MqResult<String> {
    match config.effective_ms_client_id_with_env(env_override) {
        Some(client_id) => Ok(client_id),
        None => {
            debug!("{MS_CLIENT_ID_SETUP_HINT}");
            Err(MqError::MissingClientId {
                env_var: ENV_MS_CLIENT_ID,
            })
        }
    }
}

/// Endpoint URLs for the Microsoft -> Xbox Live -> XSTS -> Minecraft Services chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsaEndpoints {
    /// Microsoft OAuth2 v2.0 device authorization endpoint.
    pub ms_device_code_url: String,
    /// Microsoft OAuth2 v2.0 token polling and refresh endpoint.
    pub ms_token_url: String,
    /// Xbox Live RPS user authentication endpoint.
    pub xbl_authenticate_url: String,
    /// Xbox Secure Token Service (XSTS) authorization endpoint.
    pub xsts_authorize_url: String,
    /// Minecraft Services Xbox identity exchange endpoint.
    pub mc_login_with_xbox_url: String,
    /// Minecraft Services game store entitlements endpoint.
    pub mc_entitlements_url: String,
    /// Minecraft Services player profile and skin metadata endpoint.
    pub mc_profile_url: String,
}

impl Default for MsaEndpoints {
    fn default() -> Self {
        Self {
            ms_device_code_url:
                "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode".to_owned(),
            ms_token_url: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
                .to_owned(),
            xbl_authenticate_url: "https://user.auth.xboxlive.com/user/authenticate".to_owned(),
            xsts_authorize_url: "https://xsts.auth.xboxlive.com/xsts/authorize".to_owned(),
            mc_login_with_xbox_url:
                "https://api.minecraftservices.com/authentication/login_with_xbox".to_owned(),
            mc_entitlements_url: "https://api.minecraftservices.com/entitlements/mcstore"
                .to_owned(),
            mc_profile_url: "https://api.minecraftservices.com/minecraft/profile".to_owned(),
        }
    }
}

impl MsaEndpoints {
    /// Overrides all Microsoft, Xbox Live, XSTS, and Minecraft Services endpoints to point
    /// under a single `base_url` (e.g. `http://127.0.0.1:<port>`) for hermetic testing.
    pub fn with_base_url(base_url: &str) -> Self {
        let base = base_url.trim_end_matches('/');
        Self {
            ms_device_code_url: format!("{base}/consumers/oauth2/v2.0/devicecode"),
            ms_token_url: format!("{base}/consumers/oauth2/v2.0/token"),
            xbl_authenticate_url: format!("{base}/user/authenticate"),
            xsts_authorize_url: format!("{base}/xsts/authorize"),
            mc_login_with_xbox_url: format!("{base}/authentication/login_with_xbox"),
            mc_entitlements_url: format!("{base}/entitlements/mcstore"),
            mc_profile_url: format!("{base}/minecraft/profile"),
        }
    }

    /// Resolves endpoints from [`AppConfig`], using [`AppConfig::mock_url`] when configured.
    pub fn from_config(config: &AppConfig) -> Self {
        match config
            .mock_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
        {
            Some(mock_base) => Self::with_base_url(mock_base),
            None => Self::default(),
        }
    }
}

fn default_poll_interval() -> u64 {
    5
}

/// Device code challenge returned to the UI/CLI so the user can authorize sign-in in a browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCodePrompt {
    /// Short human-friendly verification code entered by the user (e.g. `"ABCD-EFGH"`).
    pub user_code: String,
    /// High-entropy device polling token (redacted in `Debug` and `Display`).
    pub device_code: SecretString,
    /// Verification URL the user opens in their browser (e.g. `https://www.microsoft.com/link`).
    #[serde(alias = "verification_url")]
    pub verification_uri: String,
    /// Lifetime of the device code session in seconds.
    pub expires_in: u64,
    /// Recommended polling interval in seconds.
    #[serde(default = "default_poll_interval")]
    pub interval: u64,
    /// Localized user instruction string provided by Microsoft.
    #[serde(default)]
    pub message: String,
}

/// Asynchronous client executing the full Microsoft -> Xbox Live -> XSTS -> Minecraft Services flow.
#[derive(Debug, Clone)]
pub struct MsaClient {
    http: reqwest::Client,
    endpoints: MsaEndpoints,
    poll_time_unit: Duration,
}

impl MsaClient {
    /// Creates a new [`MsaClient`] targeting production Microsoft, Xbox, and Minecraft endpoints.
    pub fn new() -> MqResult<Self> {
        Self::with_endpoints(MsaEndpoints::default())
    }

    /// Creates a new [`MsaClient`] with custom [`MsaEndpoints`].
    pub fn with_endpoints(endpoints: MsaEndpoints) -> MqResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .timeout(DEFAULT_HTTP_TIMEOUT)
            .build()
            .map_err(|err| MqError::Network(format!("failed to build MSA HTTP client: {err}")))?;
        Ok(Self::with_client_and_endpoints(http, endpoints))
    }

    /// Creates a new [`MsaClient`] using [`AppConfig`] (honoring `mock_url` if set).
    pub fn from_config(config: &AppConfig) -> MqResult<Self> {
        Self::with_endpoints(MsaEndpoints::from_config(config))
    }

    /// Wraps an existing [`reqwest::Client`] and [`MsaEndpoints`].
    pub fn with_client_and_endpoints(http: reqwest::Client, endpoints: MsaEndpoints) -> Self {
        Self {
            http,
            endpoints,
            poll_time_unit: Duration::from_secs(1),
        }
    }

    /// Overrides the base time unit multiplied by `prompt.interval` when polling
    /// (defaults to `1s`; tests can set milliseconds for fast deterministic backoff verification).
    pub fn with_poll_time_unit(mut self, unit: Duration) -> Self {
        self.poll_time_unit = unit;
        self
    }

    /// Borrows the configured [`MsaEndpoints`].
    #[inline]
    pub fn endpoints(&self) -> &MsaEndpoints {
        &self.endpoints
    }

    /// Step 1: Initiates the Microsoft OAuth2 Device Code flow for consumer accounts.
    pub async fn start_device_code_flow(&self, client_id: &str) -> MqResult<DeviceCodePrompt> {
        let client_id = validate_client_id(client_id)?;
        let body = encode_form_urlencoded(&[("client_id", client_id), ("scope", MSA_OAUTH_SCOPE)]);

        let resp = self
            .http
            .post(&self.endpoints.ms_device_code_url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(reqwest::header::ACCEPT, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|err| MqError::Network(format!("device code request failed: {err}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|err| MqError::Network(format!("failed reading device code body: {err}")))?;

        if !status.is_success() {
            return Err(MqError::Auth(format!(
                "Microsoft device code initiation failed (HTTP {}): {}",
                status.as_u16(),
                summarize_oauth_error(&text, &[])
            )));
        }

        let mut prompt: DeviceCodePrompt = serde_json::from_str(&text)?;
        if prompt.message.is_empty() {
            prompt.message = format!(
                "Open {} and enter code {} to sign in.",
                prompt.verification_uri, prompt.user_code
            );
        }
        debug!(
            verification_uri = %prompt.verification_uri,
            expires_in = prompt.expires_in,
            interval = prompt.interval,
            "started Microsoft device code flow"
        );
        Ok(prompt)
    }

    /// Step 2..6: Polls the Microsoft token endpoint until authorized, then exchanges tokens
    /// through Xbox Live -> XSTS -> Minecraft Services (`login_with_xbox`, `/entitlements/mcstore`,
    /// `/minecraft/profile`) to construct a ready-to-save [`AccountProfile`].
    pub async fn poll_and_complete_login(
        &self,
        client_id: &str,
        prompt: &DeviceCodePrompt,
    ) -> MqResult<AccountProfile> {
        let client_id = validate_client_id(client_id)?;
        let ms_tokens = self.poll_ms_device_token(client_id, prompt).await?;
        self.complete_xbox_and_minecraft_chain(ms_tokens, None)
            .await
    }

    /// Refreshes an existing Microsoft [`AccountProfile`] using its stored `refresh_token`,
    /// preserving its [`AccountId`] and re-running the Xbox Live -> XSTS -> Minecraft chain.
    pub async fn refresh_msa_account(
        &self,
        client_id: &str,
        profile: &AccountProfile,
    ) -> MqResult<AccountProfile> {
        if profile.kind != AccountKind::Microsoft {
            return Err(MqError::Auth(format!(
                "cannot run Microsoft token refresh on {:?} account '{}'",
                profile.kind, profile.username
            )));
        }
        let client_id = validate_client_id(client_id)?;
        let refresh_token = profile
            .refresh_token
            .as_ref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                MqError::Auth(format!(
                    "Microsoft account '{}' has no refresh token; please sign in again",
                    profile.username
                ))
            })?;

        let ms_tokens = self
            .exchange_refresh_token(client_id, refresh_token)
            .await?;
        self.complete_xbox_and_minecraft_chain(ms_tokens, Some(profile))
            .await
    }

    /// Queries `/entitlements/mcstore` to check whether the bearer token holds direct store
    /// entitlements (`product_minecraft` / `game_minecraft`).
    pub async fn check_entitlements(&self, mc_access_token: &SecretString) -> MqResult<bool> {
        let auth_header = format!("Bearer {}", mc_access_token.expose_secret());
        let resp = self
            .http
            .get(&self.endpoints.mc_entitlements_url)
            .header(reqwest::header::AUTHORIZATION, auth_header)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|err| {
                let scrubbed = redact_text(&err.to_string(), &[mc_access_token.expose_secret()]);
                MqError::Network(format!("Minecraft entitlements request failed: {scrubbed}"))
            })?;

        let status = resp.status();
        let text = resp.text().await.map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[mc_access_token.expose_secret()]);
            MqError::Network(format!(
                "failed reading Minecraft entitlements body: {scrubbed}"
            ))
        })?;

        if status.as_u16() == 404 {
            // Allow graceful fallback when an environment or mock server omits /entitlements/mcstore;
            // /minecraft/profile remains the authoritative ownership & profile gate.
            debug!("mcstore entitlements endpoint returned 404; deferring ownership check to /minecraft/profile");
            return Ok(false);
        }

        if !status.is_success() {
            let scrubbed = redact_text(&text, &[mc_access_token.expose_secret()]);
            return Err(MqError::Auth(format!(
                "Minecraft entitlements check failed (HTTP {}): {}",
                status.as_u16(),
                scrubbed
            )));
        }

        let parsed: McEntitlementsResponse = serde_json::from_str(&text).map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[mc_access_token.expose_secret()]);
            MqError::Auth(format!("invalid Minecraft entitlements JSON: {scrubbed}"))
        })?;

        Ok(!parsed.items.is_empty())
    }

    async fn poll_ms_device_token(
        &self,
        client_id: &str,
        prompt: &DeviceCodePrompt,
    ) -> MqResult<MsTokenPair> {
        let mut interval_units = prompt.interval;
        let max_wait = self
            .poll_time_unit
            .saturating_mul(prompt.expires_in.max(1).min(u32::MAX as u64) as u32)
            .max(Duration::from_millis(250));
        let started = Instant::now();

        loop {
            if started.elapsed() >= max_wait {
                return Err(MqError::Auth(
                    "Microsoft device code expired before sign-in was completed".to_owned(),
                ));
            }

            let form_body = encode_form_urlencoded(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", prompt.device_code.expose_secret()),
            ]);

            let resp = self
                .http
                .post(&self.endpoints.ms_token_url)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(reqwest::header::ACCEPT, "application/json")
                .body(form_body)
                .send()
                .await
                .map_err(|err| {
                    let scrubbed =
                        redact_text(&err.to_string(), &[prompt.device_code.expose_secret()]);
                    MqError::Network(format!("Microsoft token poll failed: {scrubbed}"))
                })?;

            let status = resp.status();
            let text = resp.text().await.map_err(|err| {
                let scrubbed = redact_text(&err.to_string(), &[prompt.device_code.expose_secret()]);
                MqError::Network(format!("failed reading Microsoft token body: {scrubbed}"))
            })?;

            if status.is_success() {
                let token_resp: MsTokenSuccessResponse = serde_json::from_str(&text)?;
                if token_resp.access_token.expose_secret().is_empty() {
                    return Err(MqError::Auth(
                        "Microsoft token response returned an empty access_token".to_owned(),
                    ));
                }
                debug!("obtained Microsoft OAuth2 token via device code flow");
                return Ok(MsTokenPair {
                    access_token: token_resp.access_token,
                    refresh_token: token_resp.refresh_token.filter(|t| !t.is_empty()),
                });
            }

            if let Ok(err_resp) = serde_json::from_str::<MsOAuthErrorResponse>(&text) {
                match err_resp.error.as_str() {
                    "authorization_pending" => {
                        tokio::time::sleep(self.compute_poll_sleep(interval_units)).await;
                        continue;
                    }
                    "slow_down" => {
                        interval_units = interval_units.saturating_add(5);
                        debug!(
                            new_interval_units = interval_units,
                            "received slow_down from Microsoft token endpoint; increasing poll interval"
                        );
                        tokio::time::sleep(self.compute_poll_sleep(interval_units)).await;
                        continue;
                    }
                    "authorization_declined" => {
                        return Err(MqError::Auth(
                            "Microsoft sign-in was declined by the user".to_owned(),
                        ));
                    }
                    "expired_token" => {
                        return Err(MqError::Auth(
                            "Microsoft device code expired; please start sign-in again".to_owned(),
                        ));
                    }
                    "bad_verification_code" => {
                        return Err(MqError::Auth(
                            "invalid Microsoft device verification code".to_owned(),
                        ));
                    }
                    _ => {}
                }
            }

            return Err(MqError::Auth(format!(
                "Microsoft token polling failed (HTTP {}): {}",
                status.as_u16(),
                summarize_oauth_error(&text, &[prompt.device_code.expose_secret()])
            )));
        }
    }

    fn compute_poll_sleep(&self, interval_units: u64) -> Duration {
        if interval_units == 0 {
            self.poll_time_unit.min(Duration::from_millis(10))
        } else {
            self.poll_time_unit
                .saturating_mul(interval_units.min(u32::MAX as u64) as u32)
        }
    }

    async fn exchange_refresh_token(
        &self,
        client_id: &str,
        refresh_token: &SecretString,
    ) -> MqResult<MsTokenPair> {
        let form_body = encode_form_urlencoded(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("scope", MSA_OAUTH_SCOPE),
        ]);

        let resp = self
            .http
            .post(&self.endpoints.ms_token_url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(reqwest::header::ACCEPT, "application/json")
            .body(form_body)
            .send()
            .await
            .map_err(|err| {
                let scrubbed = redact_text(&err.to_string(), &[refresh_token.expose_secret()]);
                MqError::Network(format!(
                    "Microsoft token refresh request failed: {scrubbed}"
                ))
            })?;

        let status = resp.status();
        let text = resp.text().await.map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[refresh_token.expose_secret()]);
            MqError::Network(format!("failed reading refresh token response: {scrubbed}"))
        })?;

        if !status.is_success() {
            return Err(MqError::Auth(format!(
                "Microsoft token refresh failed (HTTP {}): {}",
                status.as_u16(),
                summarize_oauth_error(&text, &[refresh_token.expose_secret()])
            )));
        }

        let token_resp: MsTokenSuccessResponse = serde_json::from_str(&text)?;
        if token_resp.access_token.expose_secret().is_empty() {
            return Err(MqError::Auth(
                "Microsoft refresh response returned an empty access_token".to_owned(),
            ));
        }

        Ok(MsTokenPair {
            access_token: token_resp.access_token,
            refresh_token: token_resp
                .refresh_token
                .filter(|t| !t.is_empty())
                .or_else(|| Some(refresh_token.clone())),
        })
    }

    async fn complete_xbox_and_minecraft_chain(
        &self,
        ms_tokens: MsTokenPair,
        existing_profile: Option<&AccountProfile>,
    ) -> MqResult<AccountProfile> {
        // Step 3: Exchange Microsoft access token for Xbox Live (XBL) user token + uhs.
        let (xbl_token, xbl_uhs) = self.authenticate_xbox_live(&ms_tokens.access_token).await?;

        // Step 4: Exchange XBL token for XSTS token targeting Minecraft Services.
        let (xsts_token, xsts_uhs) = self.authorize_xsts(&xbl_token, &xbl_uhs).await?;

        // Step 5: Exchange (uhs, xsts_token) for a Minecraft Services access token.
        let (mc_access_token, expires_in) = self
            .login_minecraft_with_xbox(&xsts_uhs, &xsts_token)
            .await?;

        // Step 6a: Check Minecraft store entitlements (non-fatal on empty items for Game Pass).
        let has_store_entitlement = self.check_entitlements(&mc_access_token).await?;
        if !has_store_entitlement {
            debug!("no direct mcstore items reported; verifying Game Pass / ownership via /minecraft/profile");
        }

        // Step 6b: Fetch player UUID, username, and skin URL from /minecraft/profile.
        let mc_profile = self
            .fetch_minecraft_profile(&mc_access_token, has_store_entitlement)
            .await?;

        let player_uuid = Uuid::parse_str(mc_profile.id.trim()).map_err(|err| {
            MqError::Auth(format!(
                "Minecraft profile returned invalid UUID '{}': {err}",
                mc_profile.id
            ))
        })?;

        let skin_url = extract_active_skin_url(&mc_profile.skins);
        let preserved_avatar = existing_profile.and_then(|prev| {
            if prev.skin_url == skin_url {
                prev.avatar_head.clone()
            } else {
                None
            }
        });

        let expires_at =
            Utc::now() + chrono::Duration::seconds(i64::try_from(expires_in).unwrap_or(86_400));

        let account_id = existing_profile
            .map(|p| p.id)
            .unwrap_or_else(AccountId::new_v4);

        debug!(
            account_id = %account_id,
            username = %mc_profile.name,
            uuid = %player_uuid,
            "completed Microsoft -> Xbox Live -> XSTS -> Minecraft authentication"
        );

        Ok(AccountProfile {
            id: account_id,
            kind: AccountKind::Microsoft,
            username: mc_profile.name,
            uuid: player_uuid,
            access_token: mc_access_token,
            refresh_token: ms_tokens.refresh_token,
            client_token: None,
            expires_at: Some(expires_at),
            skin_url,
            avatar_head: preserved_avatar,
        })
    }

    async fn authenticate_xbox_live(
        &self,
        ms_access_token: &SecretString,
    ) -> MqResult<(SecretString, String)> {
        let rps_ticket = format!("d={}", ms_access_token.expose_secret());
        let payload = XblAuthRequest {
            properties: XblAuthProperties {
                auth_method: "RPS",
                site_name: "user.auth.xboxlive.com",
                rps_ticket: &rps_ticket,
            },
            relying_party: "http://auth.xboxlive.com",
            token_type: "JWT",
        };

        let resp = self
            .http
            .post(&self.endpoints.xbl_authenticate_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|err| {
                let scrubbed = redact_text(&err.to_string(), &[ms_access_token.expose_secret()]);
                MqError::Network(format!("Xbox Live authentication failed: {scrubbed}"))
            })?;

        let status = resp.status();
        let text = resp.text().await.map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[ms_access_token.expose_secret()]);
            MqError::Network(format!("failed reading Xbox Live response: {scrubbed}"))
        })?;

        if !status.is_success() {
            let scrubbed = redact_text(&text, &[ms_access_token.expose_secret()]);
            return Err(MqError::Auth(format!(
                "Xbox Live authentication rejected (HTTP {}): {}",
                status.as_u16(),
                scrubbed
            )));
        }

        let parsed: XboxTokenResponse = serde_json::from_str(&text).map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[ms_access_token.expose_secret()]);
            MqError::Auth(format!("invalid Xbox Live JSON response: {scrubbed}"))
        })?;

        let uhs = parsed
            .display_claims
            .xui
            .into_iter()
            .next()
            .map(|claim| claim.uhs)
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| {
                MqError::Auth("Xbox Live response missing DisplayClaims.xui[0].uhs".to_owned())
            })?;

        if parsed.token.is_empty() {
            return Err(MqError::Auth(
                "Xbox Live response returned an empty Token".to_owned(),
            ));
        }

        Ok((parsed.token, uhs))
    }

    async fn authorize_xsts(
        &self,
        xbl_token: &SecretString,
        fallback_uhs: &str,
    ) -> MqResult<(SecretString, String)> {
        let payload = XstsAuthRequest {
            properties: XstsAuthProperties {
                sandbox_id: "RETAIL",
                user_tokens: &[xbl_token.expose_secret()],
            },
            relying_party: "rp://api.minecraftservices.com/",
            token_type: "JWT",
        };

        let resp = self
            .http
            .post(&self.endpoints.xsts_authorize_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|err| {
                let scrubbed = redact_text(&err.to_string(), &[xbl_token.expose_secret()]);
                MqError::Network(format!("XSTS authorization failed: {scrubbed}"))
            })?;

        let status = resp.status();
        let text = resp.text().await.map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[xbl_token.expose_secret()]);
            MqError::Network(format!("failed reading XSTS response: {scrubbed}"))
        })?;

        if !status.is_success() {
            if let Ok(xerr) = serde_json::from_str::<XstsErrorResponse>(&text) {
                if let Some(code) = xerr.xerr {
                    return Err(MqError::Auth(explain_xsts_error(
                        code,
                        xerr.message.as_deref(),
                    )));
                }
            }
            let scrubbed = redact_text(&text, &[xbl_token.expose_secret()]);
            return Err(MqError::Auth(format!(
                "XSTS authorization rejected (HTTP {}): {}",
                status.as_u16(),
                scrubbed
            )));
        }

        let parsed: XboxTokenResponse = serde_json::from_str(&text).map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[xbl_token.expose_secret()]);
            MqError::Auth(format!("invalid XSTS JSON response: {scrubbed}"))
        })?;

        if parsed.token.is_empty() {
            return Err(MqError::Auth(
                "XSTS response returned an empty Token".to_owned(),
            ));
        }

        let uhs = parsed
            .display_claims
            .xui
            .into_iter()
            .next()
            .map(|c| c.uhs)
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| fallback_uhs.to_owned());

        Ok((parsed.token, uhs))
    }

    async fn login_minecraft_with_xbox(
        &self,
        uhs: &str,
        xsts_token: &SecretString,
    ) -> MqResult<(SecretString, u64)> {
        let identity_token = format!("XBL3.0 x={uhs};{}", xsts_token.expose_secret());
        let payload = McLoginWithXboxRequest {
            identity_token: &identity_token,
        };

        let resp = self
            .http
            .post(&self.endpoints.mc_login_with_xbox_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|err| {
                let scrubbed = redact_text(
                    &err.to_string(),
                    &[xsts_token.expose_secret(), &identity_token],
                );
                MqError::Network(format!(
                    "Minecraft login_with_xbox request failed: {scrubbed}"
                ))
            })?;

        let status = resp.status();
        let text = resp.text().await.map_err(|err| {
            let scrubbed = redact_text(
                &err.to_string(),
                &[xsts_token.expose_secret(), &identity_token],
            );
            MqError::Network(format!(
                "failed reading Minecraft login_with_xbox body: {scrubbed}"
            ))
        })?;

        if !status.is_success() {
            let scrubbed = redact_text(&text, &[xsts_token.expose_secret(), &identity_token]);
            return Err(MqError::Auth(format!(
                "Minecraft Services login_with_xbox failed (HTTP {}): {}",
                status.as_u16(),
                scrubbed
            )));
        }

        let parsed: McLoginWithXboxResponse = serde_json::from_str(&text).map_err(|err| {
            let scrubbed = redact_text(
                &err.to_string(),
                &[xsts_token.expose_secret(), &identity_token],
            );
            MqError::Auth(format!("invalid login_with_xbox JSON: {scrubbed}"))
        })?;

        if parsed.access_token.is_empty() {
            return Err(MqError::Auth(
                "Minecraft Services login_with_xbox returned an empty access_token".to_owned(),
            ));
        }

        Ok((parsed.access_token, parsed.expires_in))
    }

    async fn fetch_minecraft_profile(
        &self,
        mc_access_token: &SecretString,
        has_store_entitlement: bool,
    ) -> MqResult<McProfileResponse> {
        let auth_header = format!("Bearer {}", mc_access_token.expose_secret());
        let resp = self
            .http
            .get(&self.endpoints.mc_profile_url)
            .header(reqwest::header::AUTHORIZATION, auth_header)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|err| {
                let scrubbed = redact_text(&err.to_string(), &[mc_access_token.expose_secret()]);
                MqError::Network(format!("Minecraft profile request failed: {scrubbed}"))
            })?;

        let status = resp.status();
        let text = resp.text().await.map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[mc_access_token.expose_secret()]);
            MqError::Network(format!("failed reading Minecraft profile body: {scrubbed}"))
        })?;

        if status.as_u16() == 404 {
            warn!(
                has_store_entitlement,
                "Minecraft profile endpoint returned 404 Not Found"
            );
            let msg = if has_store_entitlement {
                "Minecraft: Java Edition is owned on this Microsoft account, but no player profile/username has been created yet at minecraft.net"
            } else {
                "this Microsoft account does not own Minecraft: Java Edition (no active license or profile found)"
            };
            return Err(MqError::Auth(msg.to_owned()));
        }

        if !status.is_success() {
            let scrubbed = redact_text(&text, &[mc_access_token.expose_secret()]);
            return Err(MqError::Auth(format!(
                "Minecraft profile fetch failed (HTTP {}): {}",
                status.as_u16(),
                scrubbed
            )));
        }

        let profile: McProfileResponse = serde_json::from_str(&text).map_err(|err| {
            let scrubbed = redact_text(&err.to_string(), &[mc_access_token.expose_secret()]);
            MqError::Auth(format!("invalid Minecraft profile JSON: {scrubbed}"))
        })?;

        if profile.name.trim().is_empty() {
            return Err(MqError::Auth(
                "Minecraft profile returned an empty player name".to_owned(),
            ));
        }

        Ok(profile)
    }
}

fn validate_client_id(client_id: &str) -> MqResult<&str> {
    let trimmed = client_id.trim();
    if trimmed.is_empty() {
        debug!("{MS_CLIENT_ID_SETUP_HINT}");
        return Err(MqError::MissingClientId {
            env_var: ENV_MS_CLIENT_ID,
        });
    }
    Ok(trimmed)
}

fn extract_active_skin_url(skins: &[McSkinEntry]) -> Option<String> {
    skins
        .iter()
        .find(|s| {
            s.state
                .as_deref()
                .is_some_and(|st| st.eq_ignore_ascii_case("ACTIVE"))
        })
        .or_else(|| skins.first())
        .map(|s| s.url.trim())
        .filter(|u| !u.is_empty())
        .map(ToOwned::to_owned)
}

fn explain_xsts_error(xerr: u64, raw_message: Option<&str>) -> String {
    match xerr {
        2148916227 => "this Xbox Live account has been banned (XErr 2148916227)".to_owned(),
        2148916233 => "this Microsoft account does not have an Xbox profile yet; sign in once at xbox.com to create a Gamertag (XErr 2148916233)".to_owned(),
        2148916235 => "Xbox Live is unavailable in this account's country or region (XErr 2148916235)".to_owned(),
        2148916236 | 2148916237 => format!(
            "adult age verification is required on this Microsoft/Xbox account (XErr {xerr})"
        ),
        2148916238 => "child Microsoft accounts (<18) must be added to a Microsoft Family group with multiplayer permissions enabled (XErr 2148916238)".to_owned(),
        other => match raw_message.map(str::trim).filter(|m| !m.is_empty()) {
            Some(msg) => format!("XSTS authorization failed (XErr {other}): {msg}"),
            None => format!("XSTS authorization failed (XErr {other})"),
        },
    }
}

fn summarize_oauth_error(raw_body: &str, secrets: &[&str]) -> String {
    if let Ok(parsed) = serde_json::from_str::<MsOAuthErrorResponse>(raw_body) {
        let combined = match parsed
            .error_description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            Some(desc) => format!("{}: {desc}", parsed.error),
            None => parsed.error,
        };
        return redact_text(&combined, secrets);
    }
    redact_text(raw_body.trim(), secrets)
}

/// Percent-encodes key-value pairs into an `application/x-www-form-urlencoded` payload.
fn encode_form_urlencoded(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (idx, (key, value)) in pairs.iter().enumerate() {
        if idx > 0 {
            out.push('&');
        }
        percent_encode_component(key, &mut out);
        out.push('=');
        percent_encode_component(value, &mut out);
    }
    out
}

fn percent_encode_component(input: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0F) as usize] as char);
        }
    }
}

struct MsTokenPair {
    access_token: SecretString,
    refresh_token: Option<SecretString>,
}

#[derive(Deserialize)]
struct MsTokenSuccessResponse {
    access_token: SecretString,
    #[serde(default)]
    refresh_token: Option<SecretString>,
}

#[derive(Deserialize)]
struct MsOAuthErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Serialize)]
struct XblAuthRequest<'a> {
    #[serde(rename = "Properties")]
    properties: XblAuthProperties<'a>,
    #[serde(rename = "RelyingParty")]
    relying_party: &'a str,
    #[serde(rename = "TokenType")]
    token_type: &'a str,
}

#[derive(Serialize)]
struct XblAuthProperties<'a> {
    #[serde(rename = "AuthMethod")]
    auth_method: &'a str,
    #[serde(rename = "SiteName")]
    site_name: &'a str,
    #[serde(rename = "RpsTicket")]
    rps_ticket: &'a str,
}

#[derive(Serialize)]
struct XstsAuthRequest<'a> {
    #[serde(rename = "Properties")]
    properties: XstsAuthProperties<'a>,
    #[serde(rename = "RelyingParty")]
    relying_party: &'a str,
    #[serde(rename = "TokenType")]
    token_type: &'a str,
}

#[derive(Serialize)]
struct XstsAuthProperties<'a> {
    #[serde(rename = "SandboxId")]
    sandbox_id: &'a str,
    #[serde(rename = "UserTokens")]
    user_tokens: &'a [&'a str],
}

#[derive(Deserialize)]
struct XboxTokenResponse {
    #[serde(rename = "Token")]
    token: SecretString,
    #[serde(rename = "DisplayClaims")]
    display_claims: XboxDisplayClaims,
}

#[derive(Deserialize)]
struct XboxDisplayClaims {
    #[serde(default)]
    xui: Vec<XboxXuiClaim>,
}

#[derive(Deserialize)]
struct XboxXuiClaim {
    uhs: String,
}

#[derive(Deserialize)]
struct XstsErrorResponse {
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
    #[serde(rename = "Message")]
    message: Option<String>,
}

#[derive(Serialize)]
struct McLoginWithXboxRequest<'a> {
    #[serde(rename = "identityToken")]
    identity_token: &'a str,
}

fn default_mc_expires_in() -> u64 {
    86_400
}

#[derive(Deserialize)]
struct McLoginWithXboxResponse {
    access_token: SecretString,
    #[serde(default = "default_mc_expires_in")]
    expires_in: u64,
}

#[derive(Deserialize)]
struct McEntitlementsResponse {
    #[serde(default)]
    items: Vec<McEntitlementItem>,
}

#[derive(Deserialize)]
struct McEntitlementItem {
    #[allow(dead_code)]
    name: String,
}

#[derive(Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<McSkinEntry>,
}

#[derive(Deserialize)]
struct McSkinEntry {
    #[serde(default)]
    state: Option<String>,
    url: String,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    #[test]
    fn resolve_ms_client_id_returns_missing_when_unset_and_succeeds_when_set() {
        let unset_cfg = AppConfig::default();
        let err = resolve_ms_client_id_with_env(&unset_cfg, None).unwrap_err();
        assert!(matches!(
            err,
            MqError::MissingClientId {
                env_var: "MQ_MS_CLIENT_ID"
            }
        ));
        assert!(err.to_string().contains("MQ_MS_CLIENT_ID"));
        assert!(ms_client_id_setup_hint().contains("MQ_MS_CLIENT_ID"));
        assert!(ms_client_id_setup_hint().contains("Offline"));
        assert!(ms_client_id_setup_hint().contains("Ely.by"));

        // Blank strings are treated as unset.
        let blank_cfg = AppConfig {
            ms_client_id: Some("   ".to_owned()),
            ..AppConfig::default()
        };
        assert!(matches!(
            resolve_ms_client_id_with_env(&blank_cfg, Some("  ")),
            Err(MqError::MissingClientId {
                env_var: "MQ_MS_CLIENT_ID"
            })
        ));

        // Succeeds when configured in AppConfig (and when env var is unset in current process).
        let configured = AppConfig {
            ms_client_id: Some("  00000000-402b-4c3a-9b11-112233445566  ".to_owned()),
            ..AppConfig::default()
        };
        if std::env::var(ENV_MS_CLIENT_ID).is_err() {
            assert!(matches!(
                resolve_ms_client_id(&unset_cfg),
                Err(MqError::MissingClientId {
                    env_var: "MQ_MS_CLIENT_ID"
                })
            ));
            let id = resolve_ms_client_id(&configured).expect("resolve from AppConfig");
            assert_eq!(id, "00000000-402b-4c3a-9b11-112233445566");
        }

        // Env override takes precedence over AppConfig.
        let from_env = resolve_ms_client_id_with_env(&configured, Some("env-override-client-id"))
            .expect("resolve from env override");
        assert_eq!(from_env, "env-override-client-id");
    }

    struct RawHttpRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: String,
    }

    async fn read_http_request(stream: &mut TcpStream) -> RawHttpRequest {
        let mut buf = Vec::with_capacity(4096);
        let mut chunk = [0u8; 1024];
        let header_end = loop {
            let n = stream.read(&mut chunk).await.expect("read request bytes");
            assert!(n > 0, "unexpected EOF reading HTTP headers");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos;
            }
        };

        let header_str = std::str::from_utf8(&buf[..header_end]).expect("UTF-8 HTTP headers");
        let mut lines = header_str.split("\r\n");
        let request_line = lines.next().expect("HTTP request line");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().expect("HTTP method").to_owned();
        let path = parts.next().expect("HTTP path").to_owned();

        let mut headers = HashMap::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
            }
        }

        let content_len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let body_start = header_end + 4;
        while buf.len() < body_start + content_len {
            let n = stream.read(&mut chunk).await.expect("read body bytes");
            assert!(n > 0, "unexpected EOF reading HTTP body");
            buf.extend_from_slice(&chunk[..n]);
        }

        let body =
            String::from_utf8(buf[body_start..body_start + content_len].to_vec()).expect("UTF-8");

        RawHttpRequest {
            method,
            path,
            headers,
            body,
        }
    }

    async fn write_json_response(stream: &mut TcpStream, status: u16, json_body: &str) {
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            _ => "Status",
        };
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            json_body.len()
        );
        stream
            .write_all(head.as_bytes())
            .await
            .expect("write response headers");
        stream
            .write_all(json_body.as_bytes())
            .await
            .expect("write response body");
        let _ = stream.shutdown().await;
    }

    fn decode_urlencoded_map(body: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for pair in body.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            map.insert(percent_decode(k), percent_decode(v));
        }
        map
    }

    fn percent_decode(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(val) = u8::from_str_radix(hex_str, 16) {
                        out.push(val);
                        i += 3;
                        continue;
                    }
                }
            } else if bytes[i] == b'+' {
                out.push(b' ');
                i += 1;
                continue;
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8(out).expect("decoded form UTF-8")
    }

    #[tokio::test]
    async fn full_msa_device_code_chain_and_refresh_against_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral mock port");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");

        let token_polls = Arc::new(AtomicUsize::new(0));
        let entitlements_hits = Arc::new(AtomicUsize::new(0));
        let polls_ref = Arc::clone(&token_polls);
        let ent_ref = Arc::clone(&entitlements_hits);

        let server_task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let polls = Arc::clone(&polls_ref);
                let ent_hits = Arc::clone(&ent_ref);

                tokio::spawn(async move {
                    let req = read_http_request(&mut stream).await;
                    match (req.method.as_str(), req.path.as_str()) {
                        ("POST", "/consumers/oauth2/v2.0/devicecode") => {
                            let form = decode_urlencoded_map(&req.body);
                            assert_eq!(
                                form.get("client_id").map(String::as_str),
                                Some("test-azure-client-id")
                            );
                            assert_eq!(
                                form.get("scope").map(String::as_str),
                                Some("XboxLive.signin offline_access")
                            );
                            write_json_response(
                                &mut stream,
                                200,
                                r#"{
                                    "user_code": "WXYZ-1234",
                                    "device_code": "secret_device_code_val_999",
                                    "verification_uri": "https://www.microsoft.com/link",
                                    "expires_in": 900,
                                    "interval": 1,
                                    "message": "Open https://www.microsoft.com/link and enter WXYZ-1234"
                                }"#,
                            )
                            .await;
                        }
                        ("POST", "/consumers/oauth2/v2.0/token") => {
                            let form = decode_urlencoded_map(&req.body);
                            assert_eq!(
                                form.get("client_id").map(String::as_str),
                                Some("test-azure-client-id")
                            );
                            match form.get("grant_type").map(String::as_str) {
                                Some("urn:ietf:params:oauth:grant-type:device_code") => {
                                    assert_eq!(
                                        form.get("device_code").map(String::as_str),
                                        Some("secret_device_code_val_999")
                                    );
                                    let attempt = polls.fetch_add(1, Ordering::SeqCst);
                                    match attempt {
                                        0 => {
                                            write_json_response(
                                                &mut stream,
                                                400,
                                                r#"{"error":"authorization_pending","error_description":"User has not yet authenticated"}"#,
                                            )
                                            .await;
                                        }
                                        1 => {
                                            write_json_response(
                                                &mut stream,
                                                400,
                                                r#"{"error":"slow_down","error_description":"Polling too fast"}"#,
                                            )
                                            .await;
                                        }
                                        _ => {
                                            write_json_response(
                                                &mut stream,
                                                200,
                                                r#"{
                                                    "token_type": "Bearer",
                                                    "expires_in": 3600,
                                                    "access_token": "ms_access_tok_initial_secret",
                                                    "refresh_token": "ms_refresh_tok_initial_secret"
                                                }"#,
                                            )
                                            .await;
                                        }
                                    }
                                }
                                Some("refresh_token") => {
                                    assert_eq!(
                                        form.get("refresh_token").map(String::as_str),
                                        Some("ms_refresh_tok_initial_secret")
                                    );
                                    write_json_response(
                                        &mut stream,
                                        200,
                                        r#"{
                                            "token_type": "Bearer",
                                            "expires_in": 3600,
                                            "access_token": "ms_access_tok_refreshed_secret",
                                            "refresh_token": "ms_refresh_tok_rotated_secret"
                                        }"#,
                                    )
                                    .await;
                                }
                                other => panic!("unexpected grant_type: {other:?}"),
                            }
                        }
                        ("POST", "/user/authenticate") => {
                            let val: serde_json::Value =
                                serde_json::from_str(&req.body).expect("parse XBL request");
                            assert_eq!(val["Properties"]["AuthMethod"], "RPS");
                            assert_eq!(val["Properties"]["SiteName"], "user.auth.xboxlive.com");
                            assert_eq!(val["RelyingParty"], "http://auth.xboxlive.com");
                            let ticket = val["Properties"]["RpsTicket"]
                                .as_str()
                                .expect("RpsTicket str");
                            assert!(
                                ticket == "d=ms_access_tok_initial_secret"
                                    || ticket == "d=ms_access_tok_refreshed_secret"
                            );
                            write_json_response(
                                &mut stream,
                                200,
                                r#"{
                                    "Token": "xbl_jwt_secret_token_abc",
                                    "DisplayClaims": {
                                        "xui": [{ "uhs": "8877665544332211" }]
                                    }
                                }"#,
                            )
                            .await;
                        }
                        ("POST", "/xsts/authorize") => {
                            let val: serde_json::Value =
                                serde_json::from_str(&req.body).expect("parse XSTS request");
                            assert_eq!(val["Properties"]["SandboxId"], "RETAIL");
                            assert_eq!(
                                val["Properties"]["UserTokens"][0],
                                "xbl_jwt_secret_token_abc"
                            );
                            assert_eq!(val["RelyingParty"], "rp://api.minecraftservices.com/");
                            write_json_response(
                                &mut stream,
                                200,
                                r#"{
                                    "Token": "xsts_jwt_secret_token_xyz",
                                    "DisplayClaims": {
                                        "xui": [{ "uhs": "8877665544332211" }]
                                    }
                                }"#,
                            )
                            .await;
                        }
                        ("POST", "/authentication/login_with_xbox") => {
                            let val: serde_json::Value =
                                serde_json::from_str(&req.body).expect("parse mc login request");
                            assert_eq!(
                                val["identityToken"],
                                "XBL3.0 x=8877665544332211;xsts_jwt_secret_token_xyz"
                            );
                            write_json_response(
                                &mut stream,
                                200,
                                r#"{
                                    "username": "8877665544332211",
                                    "roles": [],
                                    "access_token": "mc_bearer_secret_token_777",
                                    "token_type": "Bearer",
                                    "expires_in": 86400
                                }"#,
                            )
                            .await;
                        }
                        ("GET", "/entitlements/mcstore") => {
                            assert_eq!(
                                req.headers.get("authorization").map(String::as_str),
                                Some("Bearer mc_bearer_secret_token_777")
                            );
                            ent_hits.fetch_add(1, Ordering::SeqCst);
                            write_json_response(
                                &mut stream,
                                200,
                                r#"{
                                    "items": [
                                        { "name": "product_minecraft" },
                                        { "name": "game_minecraft" }
                                    ]
                                }"#,
                            )
                            .await;
                        }
                        ("GET", "/minecraft/profile") => {
                            assert_eq!(
                                req.headers.get("authorization").map(String::as_str),
                                Some("Bearer mc_bearer_secret_token_777")
                            );
                            write_json_response(
                                &mut stream,
                                200,
                                r#"{
                                    "id": "069a79f444e94726a5befca90e38aaf5",
                                    "name": "AlexMsa",
                                    "skins": [
                                        {
                                            "id": "skin-0",
                                            "state": "INACTIVE",
                                            "url": "http://textures.minecraft.net/texture/inactive_skin"
                                        },
                                        {
                                            "id": "skin-1",
                                            "state": "ACTIVE",
                                            "url": "http://textures.minecraft.net/texture/active_alex_skin"
                                        }
                                    ]
                                }"#,
                            )
                            .await;
                        }
                        other => panic!("unexpected mock HTTP route: {other:?}"),
                    }
                });
            }
        });

        let endpoints = MsaEndpoints::with_base_url(&base_url);
        let client = MsaClient::with_endpoints(endpoints)
            .expect("create MsaClient")
            .with_poll_time_unit(Duration::from_millis(2));

        let prompt = client
            .start_device_code_flow("test-azure-client-id")
            .await
            .expect("start_device_code_flow");
        assert_eq!(prompt.user_code, "WXYZ-1234");
        assert_eq!(prompt.verification_uri, "https://www.microsoft.com/link");
        assert_eq!(
            prompt.device_code.expose_secret(),
            "secret_device_code_val_999"
        );
        // Ensure Debug redacts device_code.
        let prompt_dbg = format!("{prompt:?}");
        assert!(!prompt_dbg.contains("secret_device_code_val_999"));
        assert!(prompt_dbg.contains("[REDACTED]"));

        let profile = client
            .poll_and_complete_login("test-azure-client-id", &prompt)
            .await
            .expect("poll_and_complete_login");

        assert_eq!(token_polls.load(Ordering::SeqCst), 3);
        assert_eq!(entitlements_hits.load(Ordering::SeqCst), 1);
        assert_eq!(profile.kind, AccountKind::Microsoft);
        assert_eq!(profile.username, "AlexMsa");
        assert_eq!(profile.uuid_simple(), "069a79f444e94726a5befca90e38aaf5");
        assert_eq!(
            profile.access_token.expose_secret(),
            "mc_bearer_secret_token_777"
        );
        assert_eq!(
            profile
                .refresh_token
                .as_ref()
                .map(SecretString::expose_secret),
            Some("ms_refresh_tok_initial_secret")
        );
        assert_eq!(
            profile.skin_url.as_deref(),
            Some("http://textures.minecraft.net/texture/active_alex_skin")
        );
        assert!(profile.expires_at.is_some());

        // Debug output on AccountProfile must never expose raw tokens.
        let profile_dbg = format!("{profile:?}");
        assert!(!profile_dbg.contains("mc_bearer_secret_token_777"));
        assert!(!profile_dbg.contains("ms_refresh_tok_initial_secret"));
        assert!(profile_dbg.contains("[REDACTED]"));

        // Verify refresh_msa_account preserves AccountId and rotates refresh_token.
        let refreshed = client
            .refresh_msa_account("test-azure-client-id", &profile)
            .await
            .expect("refresh_msa_account");
        assert_eq!(refreshed.id, profile.id);
        assert_eq!(refreshed.username, "AlexMsa");
        assert_eq!(
            refreshed
                .refresh_token
                .as_ref()
                .map(SecretString::expose_secret),
            Some("ms_refresh_tok_rotated_secret")
        );
        assert_eq!(entitlements_hits.load(Ordering::SeqCst), 2);

        server_task.abort();
    }
}
