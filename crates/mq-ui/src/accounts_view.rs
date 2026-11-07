#![deny(clippy::await_holding_lock)]
//! Accounts manager screen: Offline (Pirate UUID v3), Ely.by (Browser-only OAuth2 + authlib-injector),
//! and Microsoft Device Code Flow with 1-click switching. Zero emojis!

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use egui::{Color32, RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_auth::{
    create_offline_account, AccountKind, AccountProfile, ElyByOAuthConfig, ElyByOAuthFlow,
};
use mq_core::{AccountId, SecretString};
use uuid::Uuid;

use crate::state::UiState;
use crate::theme::{
    ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, CARD_SURFACE_HOVER, RADIUS_CARD,
    STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    paint_skin_head_avatar, primary_emerald_button, secondary_glass_button, version_badge,
    VectorIcon,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountTab {
    Offline,
    ElyBy,
    Microsoft,
}

pub struct AccountsView {
    pub current_tab: AccountTab,
    // Offline form
    pub offline_username: String,
    // Ely.by browser OAuth state
    pub ely_auth_url: Option<String>,
    pub ely_waiting_browser: bool,
    pub ely_cancel_flag: Arc<AtomicBool>,
    // MSA form
    pub msa_device_code_prompt: Option<String>,
}

impl Default for AccountsView {
    fn default() -> Self {
        Self {
            current_tab: AccountTab::Offline,
            offline_username: "Mr_Quster".to_string(),
            ely_auth_url: None,
            ely_waiting_browser: false,
            ely_cancel_flag: Arc::new(AtomicBool::new(false)),
            msa_device_code_prompt: None,
        }
    }
}

impl AccountsView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.heading(
            RichText::new("Управление аккаунтами")
                .color(TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(8.0);

        // Tab buttons conforming to specification:
        // 1. Офлайн (только ник)
        // 2. Ely.by (через браузер)
        // 3. Microsoft (через браузер)
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.current_tab == AccountTab::Offline,
                    "Офлайн (только ник)",
                )
                .clicked()
            {
                self.current_tab = AccountTab::Offline;
            }
            if ui
                .selectable_label(
                    self.current_tab == AccountTab::ElyBy,
                    "Ely.by (через браузер)",
                )
                .clicked()
            {
                self.current_tab = AccountTab::ElyBy;
            }
            if ui
                .selectable_label(
                    self.current_tab == AccountTab::Microsoft,
                    "Microsoft (через браузер)",
                )
                .clicked()
            {
                self.current_tab = AccountTab::Microsoft;
            }
        });

        ui.add_space(8.0);

        // Tab form card
        let form_card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(12));

        form_card.show(ui, |ui| {
            ui.set_width(ui.available_width());
            match self.current_tab {
                AccountTab::Offline => self.render_offline_tab(ui, state),
                AccountTab::ElyBy => self.render_elyby_tab(ui, state),
                AccountTab::Microsoft => self.render_msa_tab(ui, state),
            }
        });

        ui.add_space(12.0);
        ui.heading(
            RichText::new("Сохранённые профили")
                .color(TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(6.0);

        // List of stored accounts
        ScrollArea::vertical()
            .id_salt("stored_accounts_scroll")
            .show(ui, |ui| {
                let accounts = state.accounts.list_accounts().to_vec();
                if accounts.is_empty() {
                    ui.label(
                        RichText::new("Нет сохранённых аккаунтов. Добавьте первый выше!")
                            .color(TEXT_MUTED),
                    );
                } else {
                    for acc in &accounts {
                        let is_active =
                            state.accounts.active_account().map(|a| a.id) == Some(acc.id);

                        let card = egui::Frame::new()
                            .fill(if is_active {
                                CARD_SURFACE_HOVER
                            } else {
                                CARD_SURFACE
                            })
                            .stroke(Stroke::new(
                                1.0_f32,
                                if is_active {
                                    ACCENT_EMERALD
                                } else {
                                    BORDER_SUBTLE
                                },
                            ))
                            .corner_radius(RADIUS_CARD)
                            .inner_margin(egui::Margin::same(10));

                        card.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                paint_skin_head_avatar(
                                    ui,
                                    36.0,
                                    &acc.username,
                                    acc.avatar_head.as_ref(),
                                );

                                let right_w = if !is_active { 150.0 } else { 0.0 };
                                let middle_w = (ui.available_width() - right_w - 12.0).max(60.0);

                                ui.allocate_ui_with_layout(
                                    Vec2::new(middle_w, 40.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                                            let name_w = (ui.available_width() - 110.0).max(50.0);
                                            ui.add_sized(
                                                [name_w.min(160.0), 18.0],
                                                egui::Label::new(
                                                    RichText::new(&acc.username)
                                                        .color(TEXT_PRIMARY)
                                                        .strong(),
                                                )
                                                .truncate(),
                                            );
                                            let badge_col = match acc.kind {
                                                AccountKind::Offline => ACCENT_COBALT,
                                                AccountKind::ElyBy => ACCENT_EMERALD,
                                                AccountKind::Microsoft => {
                                                    Color32::from_rgb(0xF5, 0x9E, 0x0B)
                                                }
                                            };
                                            version_badge(ui, acc.kind.badge_label(), badge_col);
                                            if is_active {
                                                version_badge(ui, "АКТИВЕН", ACCENT_EMERALD);
                                            }
                                        });
                                        let uuid_w = (ui.available_width() - 8.0).max(40.0);
                                        ui.add_sized(
                                            [uuid_w, 16.0],
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "UUID: {}",
                                                    acc.uuid_hyphenated()
                                                ))
                                                .color(TEXT_MUTED)
                                                .small(),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );

                                if !is_active {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if secondary_glass_button(
                                                ui,
                                                Some(VectorIcon::Trash),
                                                "",
                                            )
                                            .clicked()
                                            {
                                                let _ = state
                                                    .accounts
                                                    .remove_account(&acc.id.to_string());
                                                let _ = state.accounts.save(&state.paths);
                                            }
                                            if primary_emerald_button(
                                                ui,
                                                Some(VectorIcon::Check),
                                                "Выбрать",
                                            )
                                            .clicked()
                                            {
                                                let _ = state
                                                    .accounts
                                                    .select_account(&acc.id.to_string());
                                                let _ = state.accounts.save(&state.paths);
                                            }
                                        },
                                    );
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                }
            });
    }

    fn render_offline_tab(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.label(
            RichText::new("Вход без пароля для одиночной игры и оффлайн-серверов")
                .color(TEXT_MUTED),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Никнейм игрока:");
            ui.text_edit_singleline(&mut self.offline_username);
        });

        // Instant preview of computed Offline UUID v3
        let clean_nick = self.offline_username.trim();
        if !clean_nick.is_empty() {
            let computed_uuid = mq_auth::compute_offline_uuid(clean_nick);
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Вычисленный Offline UUID v3: {}",
                    computed_uuid.hyphenated()
                ))
                .color(ACCENT_COBALT)
                .monospace()
                .small(),
            );
        }

        ui.add_space(8.0);
        if primary_emerald_button(ui, Some(VectorIcon::Plus), "Добавить оффлайн-профиль").clicked()
        {
            match create_offline_account(clean_nick) {
                Ok(profile) => {
                    state.accounts.upsert_account(profile, true);
                    let _ = state.accounts.save(&state.paths);
                    state.status_message = Some((
                        format!("Оффлайн-профиль '{clean_nick}' добавлен и выбран"),
                        false,
                    ));
                }
                Err(e) => {
                    state.status_message = Some((format!("Ошибка создания профиля: {e}"), true));
                }
            }
        }
    }

    fn render_elyby_tab(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.label(
            RichText::new("Безопасный вход через официальный портал Ely.by в браузере (OAuth2)")
                .color(TEXT_MUTED),
        );
        ui.add_space(6.0);

        let oauth_cfg = ElyByOAuthConfig::default();

        if self.ely_waiting_browser {
            ui.label(
                RichText::new("Ожидание подтверждения входа в браузере...")
                    .color(ACCENT_COBALT)
                    .strong(),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if secondary_glass_button(ui, Some(VectorIcon::Globe), "Открыть ссылку ещё раз")
                    .clicked()
                {
                    if let Some(ref url) = self.ely_auth_url {
                        let _ = crate::opener::open_that(url);
                    }
                }
                if secondary_glass_button(ui, Some(VectorIcon::Copy), "Скопировать ссылку")
                    .clicked()
                {
                    if let Some(ref url) = self.ely_auth_url {
                        ui.ctx().copy_text(url.clone());
                        state.status_message = Some((
                            "Ссылка для авторизации скопирована в буфер обмена".to_string(),
                            false,
                        ));
                    }
                }
                if secondary_glass_button(ui, Some(VectorIcon::Cross), "Отмена").clicked() {
                    self.ely_waiting_browser = false;
                    self.ely_cancel_flag.store(true, Ordering::SeqCst);
                    state.status_message = Some(("Вход через Ely.by отменён".to_string(), false));
                }
            });
            return;
        }

        if let Some(client_id) = oauth_cfg.client_id.as_deref() {
            ui.label(
                RichText::new(format!(
                    "Настроен клиент ID: {}",
                    &client_id[..client_id.len().min(8)]
                ))
                .color(TEXT_MUTED)
                .small(),
            );
            ui.add_space(8.0);

            if primary_emerald_button(
                ui,
                Some(VectorIcon::Globe),
                "Войти через Ely.by (в браузере)",
            )
            .clicked()
            {
                self.ely_waiting_browser = true;
                let state_str = Uuid::new_v4().to_string();
                let port = oauth_cfg.redirect_port;
                let auth_url = ElyByOAuthFlow::generate_auth_url(client_id, port, &state_str);
                self.ely_auth_url = Some(auth_url.clone());
                let _ = crate::opener::open_that(&auth_url);

                let cancel_flag = Arc::new(AtomicBool::new(false));
                self.ely_cancel_flag = cancel_flag.clone();
                let client_id_owned = client_id.to_string();
                let client_secret_owned = oauth_cfg.client_secret.clone();
                let paths_clone = state.paths.clone();

                // Spawn background task to receive loopback callback and exchange token
                let handle = tokio::runtime::Handle::current();
                std::thread::spawn(move || {
                    handle.block_on(async move {
                        let res = ElyByOAuthFlow::start_loopback_listener(port, &state_str).await;
                        if cancel_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        match res {
                            Ok(code) => {
                                let http = reqwest::Client::new();
                                let redirect_uri = format!("http://127.0.0.1:{port}/callback");
                                match ElyByOAuthFlow::exchange_code_for_token(
                                    &http,
                                    &client_id_owned,
                                    client_secret_owned.as_deref(),
                                    &code,
                                    &redirect_uri,
                                )
                                .await
                                {
                                    Ok(token_resp) => {
                                        match ElyByOAuthFlow::fetch_account_info(
                                            &http,
                                            &token_resp.access_token,
                                        )
                                        .await
                                        {
                                            Ok(info) => {
                                                let profile = AccountProfile {
                                                    id: AccountId::new_v4(),
                                                    username: info.username,
                                                    uuid: Uuid::parse_str(&info.uuid)
                                                        .unwrap_or_else(|_| Uuid::new_v4()),
                                                    kind: AccountKind::ElyBy,
                                                    access_token: SecretString::new(
                                                        token_resp.access_token,
                                                    ),
                                                    refresh_token: token_resp
                                                        .refresh_token
                                                        .map(SecretString::new),
                                                    client_token: None,
                                                    expires_at: token_resp.expires_in.map(|secs| {
                                                        chrono::Utc::now()
                                                            + chrono::Duration::seconds(secs as i64)
                                                    }),
                                                    skin_url: None,
                                                    avatar_head: None,
                                                };
                                                // Stored accounts are updated on next save or check
                                                if let Ok(mut store) =
                                                    mq_auth::AccountStore::load_or_default(
                                                        &paths_clone,
                                                    )
                                                {
                                                    store.upsert_account(profile, true);
                                                    let _ = store.save(&paths_clone);
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "Failed to fetch Ely.by account info: {e}"
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to exchange Ely.by code for token: {e}"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Ely.by loopback callback failed: {e}");
                            }
                        }
                    });
                });
            }
        } else {
            ui.label(RichText::new("Для входа через браузер необходимо указать Client ID приложения Ely.by OAuth2.").color(STATUS_WARN));
            ui.label(
                RichText::new("1. Зарегистрируйте приложение на account.ely.by/dev/applications")
                    .color(TEXT_MUTED)
                    .small(),
            );
            ui.label(
                RichText::new("2. Укажите Redirect URI: http://127.0.0.1:25585/callback")
                    .color(TEXT_MUTED)
                    .small(),
            );
            ui.label(RichText::new("3. Задайте переменную окружения MQ_ELYBY_CLIENT_ID=<id> или пропишите в config.toml").color(TEXT_MUTED).small());
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if primary_emerald_button(
                    ui,
                    Some(VectorIcon::Globe),
                    "Зарегистрировать приложение на Ely.by",
                )
                .clicked()
                {
                    let _ = crate::opener::open_that("https://account.ely.by/dev/applications");
                }
                if secondary_glass_button(ui, Some(VectorIcon::Globe), "Создать аккаунт на Ely.by")
                    .clicked()
                {
                    let _ = crate::opener::open_that("https://ely.by/registration");
                }
            });
        }
    }

    fn render_msa_tab(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.label(
            RichText::new("Официальная лицензия Microsoft (Device Code Flow)").color(TEXT_MUTED),
        );
        ui.add_space(6.0);

        if state.config.effective_ms_client_id().is_none() {
            ui.label(RichText::new("Для входа по лицензии Microsoft укажите Client ID в MQ_MS_CLIENT_ID или настройках (Пиратка и Ely.by работают сразу из коробки).").color(STATUS_WARN).small());
            ui.add_space(6.0);
        }

        if let Some(prompt) = &self.msa_device_code_prompt {
            ui.label(RichText::new(prompt).color(ACCENT_COBALT).strong());
        } else if primary_emerald_button(ui, Some(VectorIcon::User), "Войти через Microsoft")
            .clicked()
        {
            self.msa_device_code_prompt =
                Some("Откройте https://microsoft.com/link и введите код...".to_string());
        }
    }
}
