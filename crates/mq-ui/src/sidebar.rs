//! Left navigation sidebar with geometric `MQ` vector emblem, fixed 220px width,
//! 2px left accent border on active tab, and bottom 24x24 avatar profile.

use egui::{
    Align, Context, CornerRadius, FontId, Id, Layout, Painter, Pos2, Rect, RichText, Sense, Shape,
    SidePanel, Stroke, StrokeKind, Ui, Vec2,
};
use mq_auth::AccountProfile;

use crate::i18n::{tr, Language};
use crate::theme::{solid_sidebar_frame, Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

/// Fixed width of the left navigation sidebar in logical points (strictly 220px).
pub const SIDEBAR_WIDTH: f32 = 220.0;

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
    /// All primary navigation tabs in display order.
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
            Self::Catalog => "Каталог",
            Self::Accounts => "Аккаунты",
            Self::Performance => "Производительность",
            Self::Settings => "Настройки",
        }
    }

    /// Translated UI label for this navigation tab for a specific language.
    pub fn label_for(&self, lang: &str) -> &'static str {
        match self {
            Self::Instances => tr("tab_instances", lang),
            Self::Catalog => tr("tab_catalog", lang),
            Self::Accounts => tr("tab_accounts", lang),
            Self::Performance => tr("tab_performance", lang),
            Self::Settings => tr("tab_settings", lang),
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
    /// Switch the launcher interface language.
    ChangeLanguage(String),
}

/// Paints the crisp geometric `MQ` isometric cube emblem inside `rect`.
pub fn paint_mq_emblem(painter: &Painter, rect: Rect, tokens: &Tokens) {
    painter.rect_filled(rect, CornerRadius::same(6), tokens.bg_surface);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, tokens.accent.gamma_multiply(0.7)),
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
    painter.add(Shape::closed_line(hex, Stroke::new(1.5_f32, tokens.accent)));

    // Stylized "M" + isometric core lines
    let m_stroke = Stroke::new(1.8_f32, tokens.text);
    painter.line_segment([p(0.26, 0.68), p(0.26, 0.34)], m_stroke);
    painter.line_segment([p(0.26, 0.34), p(0.50, 0.54)], m_stroke);
    painter.line_segment([p(0.50, 0.54), p(0.74, 0.34)], m_stroke);
    painter.line_segment([p(0.74, 0.34), p(0.74, 0.68)], m_stroke);
    painter.line_segment(
        [p(0.56, 0.58), p(0.78, 0.78)],
        Stroke::new(1.8_f32, tokens.accent),
    );
}

fn render_nav_row(
    ui: &mut Ui,
    tab: NavTab,
    is_active: bool,
    lang_code: &str,
    tokens: &Tokens,
) -> bool {
    let available_w = ui.available_width().max(160.0);
    let row_h = 36.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available_w, row_h), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let painter = ui.painter();

        if is_active {
            let cr = CornerRadius {
                nw: 0,
                sw: 0,
                ne: 6,
                se: 6,
            };
            painter.rect_filled(rect, cr, tokens.border);
            // 2px left accent bar
            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left(), rect.top() + 4.0),
                Pos2::new(rect.left() + 2.0, rect.bottom() - 4.0),
            );
            painter.rect_filled(bar_rect, CornerRadius::same(1), tokens.accent);
        } else if hovered {
            let cr = CornerRadius {
                nw: 0,
                sw: 0,
                ne: 6,
                se: 6,
            };
            painter.rect_filled(rect, cr, tokens.bg_surface);
        }

        // Vector Icon
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 20.0, rect.center().y),
            Vec2::splat(16.0),
        );
        let icon_color = if is_active {
            tokens.accent
        } else if hovered {
            tokens.text
        } else {
            tokens.text_muted
        };
        paint_vector_icon(painter, icon_rect, tab.icon(), icon_color);

        // Label
        let text_color = if is_active || hovered {
            tokens.text
        } else {
            tokens.text_muted
        };
        let font_id = FontId::proportional(13.0);
        let galley =
            painter.layout_no_wrap(tab.label_for(lang_code).to_owned(), font_id, text_color);
        let text_pos = Pos2::new(rect.left() + 38.0, rect.center().y - galley.size().y * 0.5);
        painter.galley(text_pos, galley, text_color);
    }

    response.clicked()
}

fn render_bottom_account_profile(
    ui: &mut Ui,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
    tokens: &Tokens,
) -> Option<SidebarAction> {
    let mut action = None;
    let active_profile = active_account_id
        .and_then(|id| {
            accounts
                .iter()
                .find(|a| a.id.to_string() == id || a.username.eq_ignore_ascii_case(id))
        })
        .or_else(|| accounts.first());

    let (username, avatar_opt, kind_str) = match active_profile {
        Some(prof) => (
            prof.username.as_str(),
            prof.avatar_head.as_ref(),
            prof.kind.badge_label(),
        ),
        None => ("Гость", None, "Оффлайн"),
    };

    let popup_id = Id::new("mq_sidebar_quick_account_popup");
    let available_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(available_w, 40.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        let painter = ui.painter();

        if hovered {
            painter.rect_filled(rect, RADIUS_BUTTON, tokens.bg_surface);
            painter.rect_stroke(
                rect,
                RADIUS_BUTTON,
                Stroke::new(1.0_f32, tokens.border),
                StrokeKind::Inside,
            );
        }

        // 24x24 avatar
        let avatar_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.center().y - 12.0),
            Vec2::splat(24.0),
        );
        let fallback;
        let resolved = match avatar_opt {
            Some(av) if av.rgba.len() == mq_auth::SkinHeadAvatar::BYTE_LEN => av,
            _ => {
                fallback = mq_auth::generate_offline_geometric_avatar(username);
                &fallback
            }
        };
        crate::widgets::paint_skin_head_in_rect(painter, avatar_rect, resolved);

        // Nickname + account type
        let name_font = FontId::proportional(12.5);
        let galley = painter.layout_no_wrap(username.to_string(), name_font, tokens.text);
        let name_pos = Pos2::new(rect.left() + 36.0, rect.top() + 6.0);
        painter.galley(name_pos, galley, tokens.text);

        let kind_font = FontId::proportional(10.5);
        let kind_galley =
            painter.layout_no_wrap(kind_str.to_string(), kind_font, tokens.text_muted);
        let kind_pos = Pos2::new(rect.left() + 36.0, rect.top() + 22.0);
        painter.galley(kind_pos, kind_galley, tokens.text_muted);

        // Chevron on right
        let chev_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 14.0, rect.center().y),
            Vec2::splat(12.0),
        );
        let icon = if accounts.len() > 1 {
            VectorIcon::ChevronDown
        } else {
            VectorIcon::ChevronRight
        };
        paint_vector_icon(painter, chev_rect, icon, tokens.text_muted);
    }

    if resp.clicked() {
        if accounts.len() > 1 {
            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
        } else {
            action = Some(SidebarAction::OpenAccountsTab);
        }
    }

    if accounts.len() > 1 {
        egui::popup_above_or_below_widget(
            ui,
            popup_id,
            &resp,
            egui::AboveOrBelow::Above,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(200.0);
                ui.label(
                    RichText::new("Смена аккаунта")
                        .color(tokens.text_muted)
                        .size(11.0),
                );
                ui.separator();
                for acc in accounts {
                    let is_current = active_profile
                        .map(|curr| curr.id == acc.id)
                        .unwrap_or(false);
                    let label = format!("{} ({})", acc.username, acc.kind.badge_label());
                    if ui
                        .selectable_label(is_current, RichText::new(label).color(tokens.text))
                        .clicked()
                    {
                        action = Some(SidebarAction::SwitchAccount(acc.id.to_string()));
                        ui.memory_mut(|mem| mem.close_popup());
                    }
                }
                ui.separator();
                if ui
                    .button(RichText::new("Управление аккаунтами").color(tokens.accent))
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
    lang_code: &str,
    tokens: &Tokens,
) -> Option<SidebarAction> {
    let mut action = None;
    let lang_popup_id = Id::new("mq_sidebar_language_popup");

    // 1. Brand Header: MQ emblem + title + version 11px gray + Language button
    ui.horizontal(|ui| {
        let (emblem_rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
        if ui.is_rect_visible(emblem_rect) {
            paint_mq_emblem(ui.painter(), emblem_rect, tokens);
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(
                RichText::new("MQ LAUNCHER")
                    .color(tokens.text)
                    .size(14.0)
                    .strong(),
            );
            ui.label(RichText::new("v2.0.0").color(tokens.text_muted).size(11.0));
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (globe_rect, globe_resp) =
                ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
            if ui.is_rect_visible(globe_rect) {
                let hovered = globe_resp.hovered();
                if hovered {
                    ui.painter()
                        .rect_filled(globe_rect, RADIUS_BUTTON, tokens.bg_surface);
                }
                paint_vector_icon(
                    ui.painter(),
                    globe_rect.shrink(4.0),
                    VectorIcon::Globe,
                    if hovered {
                        tokens.accent
                    } else {
                        tokens.text_muted
                    },
                );
            }
            if globe_resp.clicked() {
                ui.memory_mut(|mem| mem.toggle_popup(lang_popup_id));
            }

            egui::popup_below_widget(
                ui,
                lang_popup_id,
                &globe_resp,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(150.0);
                    ui.label(
                        RichText::new("Язык / Language")
                            .color(tokens.text_muted)
                            .size(11.0),
                    );
                    ui.separator();
                    for lang in Language::ALL {
                        let is_current = lang_code == lang.code();
                        if ui
                            .selectable_label(is_current, lang.native_name())
                            .clicked()
                        {
                            action = Some(SidebarAction::ChangeLanguage(lang.code().to_string()));
                            ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                },
            );
        });
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // 2. Navigation Rows
    for tab in NavTab::ALL {
        if render_nav_row(ui, tab, tab == active_tab, lang_code, tokens) {
            action = Some(SidebarAction::SelectTab(tab));
        }
    }

    // 3. Bottom Mini-Profile
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if let Some(card_action) =
            render_bottom_account_profile(ui, accounts, active_account_id, tokens)
        {
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
    lang_code: &str,
    tokens: &Tokens,
) -> Option<SidebarAction> {
    SidePanel::left("mq_left_sidebar")
        .resizable(false)
        .exact_width(SIDEBAR_WIDTH)
        .frame(solid_sidebar_frame(tokens))
        .show(ctx, |ui| {
            render_sidebar(
                ui,
                active_tab,
                accounts,
                active_account_id,
                lang_code,
                tokens,
            )
        })
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_tabs_have_distinct_labels_and_icons() {
        assert_eq!(NavTab::Instances.label(), "Инстансы");
        assert_eq!(NavTab::Catalog.label(), "Каталог");
        assert_eq!(NavTab::Accounts.label(), "Аккаунты");
        assert_eq!(NavTab::Performance.label(), "Производительность");
        assert_eq!(NavTab::Settings.label(), "Настройки");
        assert_eq!(NavTab::ALL.len(), 5);
        assert_eq!(SIDEBAR_WIDTH, 220.0);
    }
}
