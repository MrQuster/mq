//! Left navigation sidebar with geometric `MQ` vector emblem, 5 primary tabs, active emerald
//! indicator bar, and bottom Active Account card with 2D Nearest-Neighbor skin avatar and
//! 1-click account switcher.

use egui::{
    Align, Color32, Context, CornerRadius, FontId, Id, Layout, Painter, Pos2, Rect, RichText,
    Sense, Shape, SidePanel, Stroke, StrokeKind, Ui, Vec2,
};
use mq_auth::{AccountKind, AccountProfile};

use crate::theme::{
    sidebar_frame, ACCENT_COBALT, ACCENT_EMERALD, BG_DEEP_GRAPHITE, BORDER_HOVER, BORDER_SUBTLE,
    CARD_SURFACE, CARD_SURFACE_HOVER, RADIUS_BUTTON, RADIUS_CARD, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{paint_skin_head_avatar, paint_vector_icon, version_badge, VectorIcon};

/// Default fixed width of the left navigation sidebar in logical points.
pub const SIDEBAR_WIDTH: f32 = 248.0;

/// Primary navigation tabs in the `mq` desktop launcher.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavTab {
    #[default]
    Instances,
    Catalog,
    Accounts,
    Performance,
    Settings,
}

impl NavTab {
    /// All 5 navigation tabs in display order.
    pub const ALL: [Self; 5] = [
        Self::Instances,
        Self::Catalog,
        Self::Accounts,
        Self::Performance,
        Self::Settings,
    ];

    /// Russian UI label for this navigation tab.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Instances => "Инстансы",
            Self::Catalog => "Каталог модов и сборок",
            Self::Accounts => "Аккаунты",
            Self::Performance => "Производительность и задачи",
            Self::Settings => "Настройки",
        }
    }

    /// Vector icon associated with this navigation tab.
    pub const fn icon(&self) -> VectorIcon {
        match self {
            Self::Instances => VectorIcon::Layers,
            Self::Catalog => VectorIcon::Catalog,
            Self::Accounts => VectorIcon::User,
            Self::Performance => VectorIcon::Cpu,
            Self::Settings => VectorIcon::Settings,
        }
    }
}

/// User interaction emitted by the left navigation sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarAction {
    /// Switch the central view to the given [`NavTab`].
    SelectTab(NavTab),
    /// Switch the active account to the given account ID (`AccountId` string).
    SwitchAccount(String),
    /// Navigate directly to the [`NavTab::Accounts`] management tab.
    OpenAccountsTab,
}

/// Paints the crisp geometric `MQ` isometric cube/monogram emblem inside `rect`.
pub fn paint_mq_emblem(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, CornerRadius::same(6), CARD_SURFACE_HOVER);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, ACCENT_EMERALD.gamma_multiply(0.65)),
        StrokeKind::Inside,
    );

    let r = rect.shrink(5.0);
    let p = |nx: f32, ny: f32| Pos2::new(r.left() + r.width() * nx, r.top() + r.height() * ny);

    // Isometric hexagon outline
    let hex = vec![
        p(0.50, 0.06),
        p(0.90, 0.28),
        p(0.90, 0.72),
        p(0.50, 0.94),
        p(0.10, 0.72),
        p(0.10, 0.28),
    ];
    painter.add(Shape::closed_line(
        hex,
        Stroke::new(1.6_f32, ACCENT_EMERALD),
    ));

    // Stylized "M" + isometric core lines
    let m_stroke = Stroke::new(1.8_f32, TEXT_PRIMARY);
    painter.line_segment([p(0.26, 0.68), p(0.26, 0.34)], m_stroke);
    painter.line_segment([p(0.26, 0.34), p(0.50, 0.54)], m_stroke);
    painter.line_segment([p(0.50, 0.54), p(0.74, 0.34)], m_stroke);
    painter.line_segment([p(0.74, 0.34), p(0.74, 0.68)], m_stroke);
    painter.line_segment(
        [p(0.56, 0.58), p(0.78, 0.78)],
        Stroke::new(1.8_f32, ACCENT_COBALT),
    );
}

fn kind_badge_color(kind: AccountKind) -> Color32 {
    match kind {
        AccountKind::Offline => TEXT_MUTED,
        AccountKind::ElyBy => ACCENT_EMERALD,
        AccountKind::Microsoft => ACCENT_COBALT,
    }
}

fn render_nav_row(ui: &mut Ui, tab: NavTab, is_active: bool) -> bool {
    let available_w = ui.available_width().max(160.0);
    let row_h = 38.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, row_h), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let painter = ui.painter();

        if is_active {
            painter.rect_filled(rect, RADIUS_BUTTON, CARD_SURFACE_HOVER);
            painter.rect_stroke(
                rect,
                RADIUS_BUTTON,
                Stroke::new(1.0_f32, ACCENT_EMERALD.gamma_multiply(0.55)),
                StrokeKind::Inside,
            );
            // Active 3px emerald indicator bar on the left edge
            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left() + 2.0, rect.top() + 7.0),
                Pos2::new(rect.left() + 5.0, rect.bottom() - 7.0),
            );
            painter.rect_filled(bar_rect, CornerRadius::same(2), ACCENT_EMERALD);
        } else if hovered {
            painter.rect_filled(rect, RADIUS_BUTTON, CARD_SURFACE_HOVER);
            painter.rect_stroke(
                rect,
                RADIUS_BUTTON,
                Stroke::new(1.0_f32, BORDER_HOVER),
                StrokeKind::Inside,
            );
        }

        let icon_color = if is_active {
            ACCENT_EMERALD
        } else if hovered {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };
        let text_color = if is_active || hovered {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };

        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 22.0, rect.center().y),
            Vec2::splat(16.0),
        );
        paint_vector_icon(painter, icon_rect, tab.icon(), icon_color);

        let galley = painter.layout_no_wrap(
            tab.label().to_owned(),
            FontId::proportional(13.5),
            text_color,
        );
        let text_pos = Pos2::new(rect.left() + 40.0, rect.center().y - galley.size().y * 0.5);
        painter.galley(text_pos, galley, text_color);
    }

    response.clicked()
}

fn render_active_account_card(
    ui: &mut Ui,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
) -> Option<SidebarAction> {
    let mut action = None;

    let active_profile = active_account_id
        .and_then(|id| {
            accounts
                .iter()
                .find(|a| a.id.to_string() == id || a.username.eq_ignore_ascii_case(id))
        })
        .or_else(|| accounts.first());

    let popup_id = Id::new("mq_sidebar_account_switcher_popup");

    let card_frame = egui::Frame::new()
        .fill(CARD_SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(RADIUS_CARD)
        .inner_margin(egui::Margin::same(10));

    let inner_resp = card_frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (username, kind, avatar_opt) = match active_profile {
                Some(prof) => (prof.username.as_str(), prof.kind, prof.avatar_head.as_ref()),
                None => ("Гость", AccountKind::Offline, None),
            };

            let avatar_resp = paint_skin_head_avatar(ui, 36.0, username, avatar_opt);
            if avatar_resp.clicked() {
                action = Some(SidebarAction::OpenAccountsTab);
            }

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(
                    RichText::new(username)
                        .color(TEXT_PRIMARY)
                        .size(13.5)
                        .strong(),
                );
                version_badge(ui, kind.badge_label(), kind_badge_color(kind));
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (btn_rect, btn_resp) =
                    ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                if ui.is_rect_visible(btn_rect) {
                    let hovered = btn_resp.hovered();
                    if hovered {
                        ui.painter()
                            .rect_filled(btn_rect, RADIUS_BUTTON, CARD_SURFACE_HOVER);
                    }
                    let icon = if accounts.len() > 1 {
                        VectorIcon::ChevronDown
                    } else {
                        VectorIcon::ChevronRight
                    };
                    paint_vector_icon(
                        ui.painter(),
                        btn_rect.shrink(5.0),
                        icon,
                        if hovered { TEXT_PRIMARY } else { TEXT_MUTED },
                    );
                }
                if btn_resp.clicked() {
                    if accounts.len() > 1 {
                        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                    } else {
                        action = Some(SidebarAction::OpenAccountsTab);
                    }
                }
            });
        });
    });

    if accounts.len() > 1 {
        egui::popup_above_or_below_widget(
            ui,
            popup_id,
            &inner_resp.response,
            egui::AboveOrBelow::Above,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(210.0);
                ui.label(
                    RichText::new("Быстрая смена аккаунта")
                        .color(TEXT_MUTED)
                        .size(11.5),
                );
                ui.separator();
                for acc in accounts {
                    let is_current = active_profile
                        .map(|curr| curr.id == acc.id)
                        .unwrap_or(false);
                    let row_resp = ui.horizontal(|ui| {
                        let _ = paint_skin_head_avatar(
                            ui,
                            20.0,
                            &acc.username,
                            acc.avatar_head.as_ref(),
                        );
                        let label_text = format!("{} ({})", acc.username, acc.kind.badge_label());
                        if ui
                            .selectable_label(
                                is_current,
                                RichText::new(label_text).color(if is_current {
                                    BG_DEEP_GRAPHITE
                                } else {
                                    TEXT_PRIMARY
                                }),
                            )
                            .clicked()
                        {
                            action = Some(SidebarAction::SwitchAccount(acc.id.to_string()));
                            ui.memory_mut(|mem| mem.close_popup());
                        }
                    });
                    let _ = row_resp;
                }
                ui.separator();
                if ui
                    .button(RichText::new("Управление аккаунтами").color(ACCENT_COBALT))
                    .clicked()
                {
                    action = Some(SidebarAction::OpenAccountsTab);
                    ui.memory_mut(|mem| mem.close_popup());
                }
            },
        );
    }

    action
}

/// Renders the sidebar contents inside an existing [`egui::Ui`].
pub fn render_sidebar(
    ui: &mut Ui,
    active_tab: NavTab,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
) -> Option<SidebarAction> {
    let mut action = None;

    // 1. Brand Header
    ui.horizontal(|ui| {
        let (emblem_rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
        if ui.is_rect_visible(emblem_rect) {
            paint_mq_emblem(ui.painter(), emblem_rect);
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                RichText::new("MQ LAUNCHER")
                    .color(TEXT_PRIMARY)
                    .size(15.0)
                    .strong(),
            );
            version_badge(ui, "v0.1.0", ACCENT_COBALT);
        });
    });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // 2. Navigation Rows
    for tab in NavTab::ALL {
        if render_nav_row(ui, tab, tab == active_tab) {
            action = Some(SidebarAction::SelectTab(tab));
        }
    }

    // 3. Bottom Active Account Card
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if let Some(card_action) = render_active_account_card(ui, accounts, active_account_id) {
            action = Some(card_action);
        }
        ui.add_space(6.0);
        ui.separator();
    });

    action
}

/// Renders the fixed-width left [`SidePanel`] and returns any triggered [`SidebarAction`].
pub fn show_sidebar(
    ctx: &Context,
    active_tab: NavTab,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
) -> Option<SidebarAction> {
    SidePanel::left("mq_left_sidebar")
        .resizable(false)
        .exact_width(SIDEBAR_WIDTH)
        .frame(sidebar_frame())
        .show(ctx, |ui| {
            render_sidebar(ui, active_tab, accounts, active_account_id)
        })
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_auth::create_offline_account;

    #[test]
    fn nav_tabs_have_distinct_russian_labels_and_icons() {
        assert_eq!(NavTab::Instances.label(), "Инстансы");
        assert_eq!(NavTab::Catalog.label(), "Каталог модов и сборок");
        assert_eq!(NavTab::Accounts.label(), "Аккаунты");
        assert_eq!(NavTab::Performance.label(), "Производительность и задачи");
        assert_eq!(NavTab::Settings.label(), "Настройки");
        assert_eq!(NavTab::ALL.len(), 5);
    }

    #[test]
    fn sidebar_renders_cleanly_with_active_account_and_multiple_profiles() {
        let ctx = Context::default();
        let acc1 = create_offline_account("TesterOne").unwrap();
        let acc2 = create_offline_account("TesterTwo").unwrap();
        let accounts = vec![acc1.clone(), acc2];

        let id_str = acc1.id.to_string();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let action = show_sidebar(ctx, NavTab::Instances, &accounts, Some(id_str.as_str()));
            assert!(action.is_none());
        });
    }
}
