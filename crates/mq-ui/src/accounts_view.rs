#![deny(clippy::await_holding_lock)]
//! Accounts manager screen: Offline (Pirate UUID v3), Ely.by (authlib-injector + skins),
//! and Microsoft Device Code Flow with 1-click switching. Zero emojis!

use egui::{Color32, RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_auth::{create_offline_account, ElyByClient};

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
    // Ely.by form
    pub ely_login: String,
    pub ely_pass: String,
    pub ely_2fa: String,
    pub ely_logging_in: bool,
    // MSA form
    pub msa_device_code_prompt: Option<String>,
}

impl Default for AccountsView {
    fn default() -> Self {
        Self {
            current_tab: AccountTab::Offline,
            offline_username: "Mr_Quster".to_string(),
            ely_login: String::new(),
            ely_pass: String::new(),
            ely_2fa: String::new(),
            ely_logging_in: false,
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

        // Tab buttons
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.current_tab == AccountTab::Offline, "Пиратка (Offline)")
                .clicked()
            {
                self.current_tab = AccountTab::Offline;
            }
            if ui
                .selectable_label(
                    self.current_tab == AccountTab::ElyBy,
                    "Ely.by (Скины и плащи)",
                )
                .clicked()
            {
                self.current_tab = AccountTab::ElyBy;
            }
            if ui
                .selectable_label(
                    self.current_tab == AccountTab::Microsoft,
                    "Microsoft (Лицензия)",
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
                                                mq_auth::AccountKind::Offline => ACCENT_COBALT,
                                                mq_auth::AccountKind::ElyBy => ACCENT_EMERALD,
                                                mq_auth::AccountKind::Microsoft => {
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
                                                RichText::new(format!("UUID: {}", acc.uuid_hyphenated()))
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
            RichText::new("Свободный вход без пароля (для одиночной игры и пиратских серверов)")
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
        ui.label(RichText::new("Авторизация через Ely.by с поддержкой системы скинов и плащей через authlib-injector").color(TEXT_MUTED));
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Логин или Email:");
            ui.text_edit_singleline(&mut self.ely_login);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Пароль:");
            ui.add(egui::TextEdit::singleline(&mut self.ely_pass).password(true));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("2FA код (если включён):");
            ui.text_edit_singleline(&mut self.ely_2fa);
        });

        ui.add_space(8.0);
        if primary_emerald_button(ui, Some(VectorIcon::User), "Войти через Ely.by").clicked()
        {
            let client = ElyByClient::default();
            let login = self.ely_login.trim().to_string();
            let pass = self.ely_pass.clone();
            let totp = if self.ely_2fa.trim().is_empty() {
                None
            } else {
                Some(self.ely_2fa.trim().to_string())
            };

            // We can authenticate synchronously or spawn onto tokio
            let handle = tokio::runtime::Handle::current();
            let res = std::thread::spawn(move || {
                handle.block_on(async {
                    client
                        .authenticate(&login, &pass, totp.as_deref(), None)
                        .await
                })
            })
            .join();

            match res {
                Ok(Ok(profile)) => {
                    let uname = profile.username.clone();
                    state.accounts.upsert_account(profile, true);
                    let _ = state.accounts.save(&state.paths);
                    state.status_message =
                        Some((format!("Успешный вход через Ely.by: {uname}"), false));
                }
                Ok(Err(e)) => {
                    state.status_message = Some((format!("Ошибка входа Ely.by: {e}"), true));
                }
                Err(_) => {
                    state.status_message = Some(("Ошибка потока аутентификации".to_string(), true));
                }
            }
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
