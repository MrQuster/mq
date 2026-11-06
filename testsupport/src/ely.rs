//! Ely.by Yggdrasil authentication, 64x64 RGBA skin PNG, and `authlib-injector` mock endpoints.

use std::collections::HashMap;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::payload::{build_skin_png_bytes, sha1_bytes, sha1_hex};
use crate::world::MockWorld;

/// Expected 6-digit TOTP code for the synthetic `"user2fa"` account.
pub const MOCK_TOTP_CODE: &str = "123456";

#[derive(Debug, Clone)]
struct AuthSession {
    username: String,
    profile_id: String,
    client_token: String,
}

/// In-memory session registry for stateful `/authenticate -> /validate -> /refresh` lifecycles.
#[derive(Debug, Default)]
pub(crate) struct ElyAuthStore {
    inner: Mutex<ElyAuthStoreInner>,
}

#[derive(Debug, Default)]
struct ElyAuthStoreInner {
    next_seq: u64,
    sessions: HashMap<String, AuthSession>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthenticateRequest {
    pub username: String,
    pub password: String,
    pub client_token: Option<String>,
    pub request_user: Option<bool>,
    pub totp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RefreshRequest {
    pub access_token: String,
    pub client_token: Option<String>,
    pub request_user: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ValidateRequest {
    pub access_token: String,
    pub client_token: Option<String>,
}

impl ElyAuthStore {
    pub fn authenticate(&self, req: AuthenticateRequest) -> Response {
        let username = req.username.trim().to_string();
        if username.is_empty() || req.password.is_empty() {
            return yggdrasil_error(
                StatusCode::UNAUTHORIZED,
                "ForbiddenOperationException",
                "Invalid credentials. Invalid username or password.",
            );
        }

        // Support both explicit `totp` JSON field and Ely.by's `password:totp` convention.
        let (effective_password, inline_totp) =
            if let Some((pw, code)) = req.password.rsplit_once(':') {
                if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
                    (pw, Some(code))
                } else {
                    (req.password.as_str(), None)
                }
            } else {
                (req.password.as_str(), None)
            };

        if effective_password.is_empty() || effective_password == "wrong" {
            return yggdrasil_error(
                StatusCode::UNAUTHORIZED,
                "ForbiddenOperationException",
                "Invalid credentials. Invalid username or password.",
            );
        }

        if username.eq_ignore_ascii_case("user2fa") {
            let provided_totp = req.totp.as_deref().or(inline_totp);
            if provided_totp != Some(MOCK_TOTP_CODE) {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": "ForbiddenOperationException",
                        "errorMessage": "Account protected with two factor auth.",
                        "cause": "TwoFactorAuthRequired"
                    })),
                )
                    .into_response();
            }
        }

        let profile_id = deterministic_profile_id(&username);
        let client_token = req
            .client_token
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("client_{profile_id}"));

        let access_token = {
            let mut guard = self.inner.lock();
            guard.next_seq += 1;
            let seq = guard.next_seq;
            let token = format!("ely_tok_{seq}_{profile_id}");
            guard.sessions.insert(
                token.clone(),
                AuthSession {
                    username: username.clone(),
                    profile_id: profile_id.clone(),
                    client_token: client_token.clone(),
                },
            );
            token
        };

        let include_user = req.request_user.unwrap_or(true);
        Json(render_yggdrasil_auth_response(
            &access_token,
            &client_token,
            &profile_id,
            &username,
            include_user,
        ))
        .into_response()
    }

    pub fn refresh(&self, req: RefreshRequest) -> Response {
        let rotated = {
            let mut guard = self.inner.lock();
            let Some(existing) = guard.sessions.get(&req.access_token).cloned() else {
                return yggdrasil_error(
                    StatusCode::FORBIDDEN,
                    "ForbiddenOperationException",
                    "Invalid token.",
                );
            };
            if let Some(ct) = req.client_token.as_deref().filter(|s| !s.is_empty()) {
                if ct != existing.client_token {
                    return yggdrasil_error(
                        StatusCode::FORBIDDEN,
                        "ForbiddenOperationException",
                        "Client token mismatch.",
                    );
                }
            }
            guard.sessions.remove(&req.access_token);
            guard.next_seq += 1;
            let seq = guard.next_seq;
            let new_token = format!("ely_tok_{seq}_{}", existing.profile_id);
            guard.sessions.insert(new_token.clone(), existing.clone());
            (new_token, existing)
        };

        let (new_access_token, session) = rotated;
        let include_user = req.request_user.unwrap_or(true);
        Json(render_yggdrasil_auth_response(
            &new_access_token,
            &session.client_token,
            &session.profile_id,
            &session.username,
            include_user,
        ))
        .into_response()
    }

    pub fn validate(&self, req: ValidateRequest) -> Response {
        let is_valid = {
            let guard = self.inner.lock();
            match guard.sessions.get(&req.access_token) {
                Some(session) => req
                    .client_token
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|ct| ct == session.client_token)
                    .unwrap_or(true),
                None => false,
            }
        };

        if is_valid {
            StatusCode::NO_CONTENT.into_response()
        } else {
            yggdrasil_error(
                StatusCode::FORBIDDEN,
                "ForbiddenOperationException",
                "Invalid token.",
            )
        }
    }
}

pub(crate) fn serve_skin_png(username_png: &str) -> Response {
    let username = username_png
        .strip_suffix(".png")
        .unwrap_or(username_png)
        .trim();
    if username.is_empty() || username.eq_ignore_ascii_case("missing") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let png = build_skin_png_bytes(username);
    (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], png).into_response()
}

pub(crate) fn serve_authlib_latest_json(world: &MockWorld) -> Response {
    Json(json!({
        "build_number": 53,
        "version": "1.2.5",
        "release_time": "2024-06-01T00:00:00Z",
        "download_url": format!("{}/download/authlib-injector.jar", world.base_url()),
        "checksums": {
            "sha256": world.authlib_injector_sha256_hex()
        }
    }))
    .into_response()
}

pub(crate) fn serve_ely_yggdrasil_meta() -> Response {
    Json(json!({
        "meta": {
            "serverName": "Ely.by Mock",
            "implementationName": "mq-testsupport",
            "implementationVersion": "1.0.0"
        },
        "skinDomains": ["127.0.0.1", "localhost", ".ely.by"],
        "signaturePublickey": "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAKrMockKeyForOfflineTestsOnly0000000000000000000000000000000000000000000000000000000000CAwEAAQ==\n-----END PUBLIC KEY-----\n"
    }))
    .into_response()
}

fn deterministic_profile_id(username: &str) -> String {
    let digest = sha1_hex(&sha1_bytes(format!("ElyPlayer:{username}").as_bytes()));
    digest[..32].to_string()
}

fn render_yggdrasil_auth_response(
    access_token: &str,
    client_token: &str,
    profile_id: &str,
    username: &str,
    include_user: bool,
) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("accessToken".to_string(), json!(access_token));
    obj.insert("clientToken".to_string(), json!(client_token));
    obj.insert(
        "availableProfiles".to_string(),
        json!([
            {
                "id": profile_id,
                "name": username
            }
        ]),
    );
    obj.insert(
        "selectedProfile".to_string(),
        json!({
            "id": profile_id,
            "name": username
        }),
    );
    if include_user {
        obj.insert(
            "user".to_string(),
            json!({
                "id": profile_id,
                "username": username,
                "properties": [
                    {
                        "name": "preferredLanguage",
                        "value": "en"
                    }
                ]
            }),
        );
    }
    Value::Object(obj)
}

fn yggdrasil_error(status: StatusCode, error: &str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": error,
            "errorMessage": message
        })),
    )
        .into_response()
}
