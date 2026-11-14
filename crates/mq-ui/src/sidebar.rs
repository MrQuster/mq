//! Left navigation sidebar with geometric `MQ` vector emblem, responsive 224px / 64px
//! animated width, grouped navigation (Библиотека / Активность / Система), Lucide-style
//! 16px vector icons, keyboard navigation (`Ctrl+1..6`, arrows, `Enter`), and bottom
//! 28x28 account profile block.

use egui::{
    Align, Color32, Context, CornerRadius, FontId, Id, Key, Layout, Painter, Pos2, Rect, RichText,
    Sense, Shape, SidePanel, Stroke, StrokeKind, Ui, Vec2,
};
use mq_auth::{AccountKind, AccountProfile};

use crate::i18n::{tr, Language};
use crate::theme::{solid_sidebar_frame, Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

/// Expanded width of the left navigation sidebar in logical points.
pub const SIDEBAR_WIDTH: f32 = 224.0;
/// Compact width of the left navigation sidebar when collapsed or window is narrow (< 1000px).
pub const COMPACT_SIDEBAR_WIDTH: f32 = 64.0;
/// Window width threshold below which the sidebar auto-collapses to compact mode.
pub const AUTO_COLLAPSE_WINDOW_WIDTH: f32 = 1000.0;
/// Collapse/expand animation duration in seconds (~150ms).
pub const SIDEBAR_ANIMATION_SECS: f32 = 0.15;
/// Fixed height of the brand header in logical points.
pub const SIDEBAR_HEADER_HEIGHT: f32 = 56.0;
/// Height of each navigation menu row.
pub const NAV_ROW_HEIGHT: f32 = 36.0;
/// Horizontal inner padding of a navigation menu row.
pub const NAV_ROW_PADDING_X: f32 = 12.0;
/// Size of each Lucide-style navigation icon.
pub const NAV_ICON_SIZE: f32 = 16.0;
/// Gap between icon and label in a navigation row.
pub const NAV_ICON_TEXT_GAP: f32 = 12.0;
/// Corner radius of a navigation row.
pub const NAV_ROW_RADIUS: u8 = 6;
/// Size of the skin head avatar in the bottom account block.
pub const ACCOUNT_AVATAR_SIZE: f32 = 28.0;

const COLLAPSE_STATE_ID: &str = "mq_sidebar_collapse_state";
const COLLAPSE_ANIM_ID: &str = "mq_sidebar_compact_anim";

/// Persistent user preference for sidebar compact/expanded mode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SidebarCollapseState {
    /// `Some(true)` = user manually collapsed, `Some(false)` = user manually expanded,
    /// `None` = follow auto-collapse threshold (`< 1000 px`).
    pub manual_compact: Option<bool>,
}

/// Resolves whether the sidebar should be compact given window width and manual override.
#[inline]
pub fn resolve_compact_mode(window_width: f32, manual_compact: Option<bool>) -> bool {
    manual_compact.unwrap_or(window_width < AUTO_COLLAPSE_WINDOW_WIDTH)
}

/// Returns whether the sidebar is currently in compact mode for `ctx`.
pub fn is_sidebar_compact(ctx: &Context) -> bool {
    let window_width = ctx.screen_rect().width();
    let state = ctx
        .data(|d| d.get_temp::<SidebarCollapseState>(Id::new(COLLAPSE_STATE_ID)))
        .unwrap_or_default();
    resolve_compact_mode(window_width, state.manual_compact)
}

/// Toggles the user's manual sidebar collapse preference in `ctx` memory.
pub fn toggle_sidebar_compact(ctx: &Context) {
    let current = is_sidebar_compact(ctx);
    ctx.data_mut(|d| {
        d.insert_temp(
            Id::new(COLLAPSE_STATE_ID),
            SidebarCollapseState {
                manual_compact: Some(!current),
            },
        );
    });
}

/// Returns the animated width for the left navigation sidebar (`224 px` <-> `64 px`, `~150 ms`).
pub fn sidebar_width(ctx: &Context) -> f32 {
    let compact = is_sidebar_compact(ctx);
    let t = ctx.animate_bool_with_time(Id::new(COLLAPSE_ANIM_ID), compact, SIDEBAR_ANIMATION_SECS);
    egui::lerp(SIDEBAR_WIDTH..=COMPACT_SIDEBAR_WIDTH, t)
}

/// Navigation groups in the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavGroup {
    Library,
    Activity,
    System,
}

impl NavGroup {
    /// Russian sentence-case group heading.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Library => "Библиотека",
            Self::Activity => "Активность",
            Self::System => "Система",
        }
    }

    /// Tabs belonging to this group in display order.
    pub const fn tabs(&self) -> &'static [NavTab] {
        match self {
            Self::Library => &[NavTab::Instances, NavTab::Catalog],
            Self::Activity => &[NavTab::Downloads, NavTab::Performance],
            Self::System => &[NavTab::Accounts, NavTab::Settings],
        }
    }
}

/// Primary navigation tabs in the `mq` desktop launcher.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavTab {
    #[default]
    Instances,
    Catalog,
    Downloads,
    Performance,
    Accounts,
    Settings,
}

impl NavTab {
    /// All primary navigation tabs in display order.
    pub const ALL: [Self; 6] = [
        Self::Instances,
        Self::Catalog,
        Self::Downloads,
        Self::Performance,
        Self::Accounts,
        Self::Settings,
    ];

    /// Russian UI label for this navigation tab.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Instances => "Инстансы",
            Self::Catalog => "Каталог",
            Self::Downloads => "Загрузки",
            Self::Performance => "Производительность",
            Self::Accounts => "Аккаунты",
            Self::Settings => "Настройки",
        }
    }

    /// Translated UI label for this navigation tab for a specific language.
    pub fn label_for(&self, lang: &str) -> &'static str {
        match self {
            Self::Instances => tr("tab_instances", lang),
            Self::Catalog => match Language::from_code(lang) {
                Language::Russian => "Каталог",
                Language::Ukrainian => "Каталог",
                Language::English => "Catalog",
                _ => tr("tab_catalog", lang),
            },
            Self::Downloads => match Language::from_code(lang) {
                Language::Ukrainian => "Завантаження",
                Language::English => "Downloads",
                _ => "Загрузки",
            },
            Self::Performance => tr("tab_performance", lang),
            Self::Accounts => tr("tab_accounts", lang),
            Self::Settings => tr("tab_settings", lang),
        }
    }

    /// Vector icon associated with this navigation tab (16px Lucide geometry).
    pub const fn icon(&self) -> VectorIcon {
        match self {
            Self::Instances => VectorIcon::Layers,
            Self::Catalog => VectorIcon::Catalog,
            Self::Downloads => VectorIcon::Download,
            Self::Performance => VectorIcon::Activity,
            Self::Accounts => VectorIcon::User,
            Self::Settings => VectorIcon::Settings,
        }
    }

    /// Keyboard shortcut index (`1..=6`) for `Ctrl+1..6`.
    pub const fn shortcut_index(&self) -> usize {
        match self {
            Self::Instances => 1,
            Self::Catalog => 2,
            Self::Downloads => 3,
            Self::Performance => 4,
            Self::Accounts => 5,
            Self::Settings => 6,
        }
    }

    /// Human-readable shortcut label for tooltips in compact mode.
    pub const fn shortcut_label(&self) -> &'static str {
        match self {
            Self::Instances => "Ctrl+1",
            Self::Catalog => "Ctrl+2",
            Self::Downloads => "Ctrl+3",
            Self::Performance => "Ctrl+4",
            Self::Accounts => "Ctrl+5",
            Self::Settings => "Ctrl+6",
        }
    }
}

/// Optional dynamic indicators shown in the sidebar.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SidebarState {
    /// Number of active download/install tasks (> 0 shows a neutral counter on «Загрузки»).
    pub active_downloads: usize,
    /// Whether a launcher update is available (shows a neutral dot + «Доступно обновление»).
    pub update_available: bool,
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

/// Maps `AccountKind` to the human-readable source label shown in the bottom account block.
pub const fn account_source_label(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::ElyBy => "Ely.by",
        AccountKind::Microsoft => "Microsoft",
        AccountKind::Offline => "Офлайн",
    }
}

/// Paints the minimalist geometric `MQ` emblem inside `rect` (clean squircle + `Q` frame + `M` glyph).
pub fn paint_mq_emblem(painter: &Painter, rect: Rect, tokens: &Tokens) {
    let rx = (rect.width() * 0.22).round().max(4.0);
    let corner = CornerRadius::same(rx as u8);
    painter.rect_filled(rect, corner, tokens.bg_surface);
    painter.rect_stroke(
        rect,
        corner,
        Stroke::new(1.0_f32, tokens.border_strong),
        StrokeKind::Inside,
    );

    let p = |nx: f32, ny: f32| {
        Pos2::new(
            rect.left() + rect.width() * nx,
            rect.top() + rect.height() * ny,
        )
    };

    // Outer geometric `Q` rounded-square ring + bottom-right diagonal tail
    let q_rect = Rect::from_min_max(p(0.19, 0.19), p(0.79, 0.79));
    let q_stroke_w = (rect.width() * 0.085).clamp(1.6, 4.0);
    painter.rect_stroke(
        q_rect,
        CornerRadius::same((rect.width() * 0.14).round().max(3.0) as u8),
        Stroke::new(q_stroke_w, tokens.accent),
        StrokeKind::Middle,
    );
    painter.line_segment(
        [p(0.69, 0.69), p(0.85, 0.85)],
        Stroke::new((rect.width() * 0.095).clamp(1.8, 4.4), tokens.accent),
    );

    // Centered crisp geometric `M` inside the `Q` frame
    let m_stroke = Stroke::new((rect.width() * 0.082).clamp(1.6, 3.8), tokens.text);
    let m_pts = vec![
        p(0.34, 0.61),
        p(0.34, 0.35),
        p(0.49, 0.50),
        p(0.64, 0.35),
        p(0.64, 0.61),
    ];
    painter.add(Shape::line(m_pts, m_stroke));
}

/// Builds a `64x64` antialiased window/taskbar icon matching the minimalist geometric `MQ` emblem.
pub fn build_app_icon() -> egui::IconData {
    const SIZE: usize = 64;
    const SS: usize = 4; // 4x4 supersampling for crisp subpixel edges

    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    let bg = [0x12u8, 0x15, 0x1B];
    let border = [0x2A, 0x31, 0x3E];
    let accent = [0x4C, 0x9A, 0x82];
    let fg = [0xE6, 0xE8, 0xEE];

    fn sd_rounded_box(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
        let qx = (px - cx).abs() - (hx - r);
        let qy = (py - cy).abs() - (hy - r);
        let ax = qx.max(0.0);
        let ay = qy.max(0.0);
        (ax * ax + ay * ay).sqrt() + qx.max(qy).min(0.0) - r
    }

    fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
        let pax = px - ax;
        let pay = py - ay;
        let bax = bx - ax;
        let bay = by - ay;
        let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
        let dx = pax - bax * h;
        let dy = pay - bay * h;
        (dx * dx + dy * dy).sqrt()
    }

    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut acc_r = 0.0_f32;
            let mut acc_g = 0.0_f32;
            let mut acc_b = 0.0_f32;
            let mut acc_a = 0.0_f32;

            for sy in 0..SS {
                for sx in 0..SS {
                    let nx = ((x * SS + sx) as f32 + 0.5) / ((SIZE * SS) as f32);
                    let ny = ((y * SS + sy) as f32 + 0.5) / ((SIZE * SS) as f32);

                    let d_outer = sd_rounded_box(nx, ny, 0.5, 0.5, 0.47, 0.47, 0.11);
                    if d_outer > 0.0 {
                        continue;
                    }

                    let mut col = if d_outer > -0.022 { border } else { bg };

                    let d_q_box =
                        sd_rounded_box(nx, ny, 0.49, 0.49, 0.295, 0.295, 0.085).abs() - 0.044;
                    let d_q_tail = sd_segment(nx, ny, 0.69, 0.69, 0.85, 0.85) - 0.048;
                    if d_q_box.min(d_q_tail) <= 0.0 {
                        col = accent;
                    }

                    let m_r = 0.041;
                    let d_m = sd_segment(nx, ny, 0.34, 0.61, 0.34, 0.35)
                        .min(sd_segment(nx, ny, 0.34, 0.35, 0.49, 0.50))
                        .min(sd_segment(nx, ny, 0.49, 0.50, 0.64, 0.35))
                        .min(sd_segment(nx, ny, 0.64, 0.35, 0.64, 0.61))
                        - m_r;
                    if d_m <= 0.0 {
                        col = fg;
                    }

                    acc_r += col[0] as f32;
                    acc_g += col[1] as f32;
                    acc_b += col[2] as f32;
                    acc_a += 255.0;
                }
            }

            let samples = (SS * SS) as f32;
            let idx = (y * SIZE + x) * 4;
            if acc_a > 0.0 {
                rgba[idx] = (acc_r / acc_a * 255.0).round() as u8;
                rgba[idx + 1] = (acc_g / acc_a * 255.0).round() as u8;
                rgba[idx + 2] = (acc_b / acc_a * 255.0).round() as u8;
                rgba[idx + 3] = (acc_a / samples).round() as u8;
            }
        }
    }

    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

#[inline]
fn nav_item_id(tab: NavTab) -> Id {
    Id::new(("mq_sidebar_nav_item", tab.shortcut_index()))
}

fn paint_horizontal_divider(ui: &mut Ui, color: Color32) {
    let w = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, color);
    }
}

fn render_group_heading(ui: &mut Ui, label: &str, is_compact: bool, tokens: &Tokens) {
    if is_compact {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(NAV_ROW_PADDING_X);
        ui.label(RichText::new(label).size(11.0).color(tokens.text_muted));
    });
    ui.add_space(4.0);
}

fn truncate_text_to_width(
    painter: &Painter,
    text: &str,
    font_id: &FontId,
    max_width: f32,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let full = painter.layout_no_wrap(text.to_owned(), font_id.clone(), color);
    if full.size().x <= max_width || max_width <= 8.0 {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    let mut best = String::from("…");
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let mut candidate: String = chars[..mid].iter().collect();
        candidate.push('…');
        let g = painter.layout_no_wrap(candidate.clone(), font_id.clone(), color);
        if g.size().x <= max_width {
            best = candidate;
            lo = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            hi = mid - 1;
        }
    }
    painter.layout_no_wrap(best, font_id.clone(), color)
}

fn render_nav_row(
    ui: &mut Ui,
    tab: NavTab,
    is_active: bool,
    is_compact: bool,
    badge_count: Option<usize>,
    lang_code: &str,
    tokens: &Tokens,
) -> bool {
    let available_w = ui
        .available_width()
        .max(if is_compact { 40.0 } else { 140.0 });
    let id = nav_item_id(tab);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available_w, NAV_ROW_HEIGHT), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());

    let has_focus = response.has_focus();
    let hovered = response.hovered();

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let cr = CornerRadius::same(NAV_ROW_RADIUS);

        if is_active {
            painter.rect_filled(rect, cr, tokens.bg_surface);
            let bar_rect = Rect::from_min_max(
                Pos2::new(rect.left(), rect.top() + 6.0),
                Pos2::new(rect.left() + 2.0, rect.bottom() - 6.0),
            );
            painter.rect_filled(bar_rect, CornerRadius::same(1), tokens.accent);
        } else if hovered {
            painter.rect_filled(rect, cr, tokens.bg_surface_hover);
        }

        if has_focus {
            painter.rect_stroke(
                rect,
                cr,
                Stroke::new(2.0_f32, tokens.accent),
                StrokeKind::Inside,
            );
        }

        let fg_color = if is_active || hovered {
            tokens.text
        } else {
            tokens.text_muted
        };

        if is_compact {
            let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(NAV_ICON_SIZE));
            paint_vector_icon(painter, icon_rect, tab.icon(), fg_color);

            if let Some(count) = badge_count.filter(|&c| c > 0) {
                let badge_center = Pos2::new(rect.right() - 8.0, rect.top() + 8.0);
                painter.circle_filled(badge_center, 4.0, tokens.border_strong);
                let _ = count;
            }
        } else {
            let icon_left = rect.left() + NAV_ROW_PADDING_X;
            let icon_rect = Rect::from_min_size(
                Pos2::new(icon_left, rect.center().y - NAV_ICON_SIZE * 0.5),
                Vec2::splat(NAV_ICON_SIZE),
            );
            paint_vector_icon(painter, icon_rect, tab.icon(), fg_color);

            let badge_reserve = if badge_count.is_some_and(|c| c > 0) {
                30.0
            } else {
                0.0
            };
            let text_left = icon_left + NAV_ICON_SIZE + NAV_ICON_TEXT_GAP;
            let max_text_w =
                (rect.right() - NAV_ROW_PADDING_X - badge_reserve - text_left).max(20.0);
            let font_id = FontId::proportional(13.0);
            let galley = truncate_text_to_width(
                painter,
                tab.label_for(lang_code),
                &font_id,
                max_text_w,
                fg_color,
            );
            let text_pos = Pos2::new(text_left, rect.center().y - galley.size().y * 0.5);
            painter.galley(text_pos, galley, fg_color);

            if let Some(count) = badge_count.filter(|&c| c > 0) {
                let badge_text = if count > 99 {
                    "99+".to_owned()
                } else {
                    count.to_string()
                };
                let badge_font = FontId::proportional(10.5);
                let badge_galley = painter.layout_no_wrap(badge_text, badge_font, tokens.text);
                let pill_w = (badge_galley.size().x + 10.0).max(18.0);
                let pill_h = 16.0;
                let pill_rect = Rect::from_min_size(
                    Pos2::new(
                        rect.right() - NAV_ROW_PADDING_X - pill_w,
                        rect.center().y - pill_h * 0.5,
                    ),
                    Vec2::new(pill_w, pill_h),
                );
                painter.rect_filled(pill_rect, CornerRadius::same(8), tokens.bg_surface_hover);
                painter.rect_stroke(
                    pill_rect,
                    CornerRadius::same(8),
                    Stroke::new(1.0_f32, tokens.border),
                    StrokeKind::Inside,
                );
                let b_pos = Pos2::new(
                    pill_rect.center().x - badge_galley.size().x * 0.5,
                    pill_rect.center().y - badge_galley.size().y * 0.5,
                );
                painter.galley(b_pos, badge_galley, tokens.text);
            }
        }
    }

    let activated_by_enter =
        has_focus && ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space));

    let resp = if is_compact {
        response.on_hover_text(format!(
            "{} ({})",
            tab.label_for(lang_code),
            tab.shortcut_label()
        ))
    } else {
        response
    };

    resp.clicked() || activated_by_enter
}

fn handle_keyboard_navigation(ui: &mut Ui) -> Option<NavTab> {
    // Ctrl+1..6 (or Cmd+1..6) direct tab switching
    let shortcut_tab = ui.input(|i| {
        if !(i.modifiers.ctrl || i.modifiers.command) {
            return None;
        }
        let keys = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
        ];
        for (idx, key) in keys.into_iter().enumerate() {
            if i.key_pressed(key) {
                return NavTab::ALL.get(idx).copied();
            }
        }
        None
    });

    if let Some(tab) = shortcut_tab {
        ui.memory_mut(|m| m.request_focus(nav_item_id(tab)));
        return Some(tab);
    }

    None
}

fn render_header(ui: &mut Ui, is_compact: bool, tokens: &Tokens) {
    let available_w = ui.available_width().max(40.0);
    let (header_rect, _) = ui.allocate_exact_size(
        Vec2::new(available_w, SIDEBAR_HEADER_HEIGHT),
        Sense::hover(),
    );

    if is_compact {
        let emblem_rect = Rect::from_center_size(
            Pos2::new(header_rect.center().x, header_rect.top() + 14.0),
            Vec2::splat(24.0),
        );
        paint_mq_emblem(ui.painter(), emblem_rect, tokens);

        let toggle_rect = Rect::from_center_size(
            Pos2::new(header_rect.center().x, header_rect.bottom() - 12.0),
            Vec2::splat(22.0),
        );
        let toggle_resp = ui.interact(
            toggle_rect,
            Id::new("mq_sidebar_collapse_toggle"),
            Sense::click(),
        );
        if ui.is_rect_visible(toggle_rect) {
            if toggle_resp.hovered() {
                ui.painter().rect_filled(
                    toggle_rect,
                    CornerRadius::same(NAV_ROW_RADIUS),
                    tokens.bg_surface_hover,
                );
            }
            paint_vector_icon(
                ui.painter(),
                Rect::from_center_size(toggle_rect.center(), Vec2::splat(16.0)),
                VectorIcon::ChevronsRight,
                if toggle_resp.hovered() {
                    tokens.text
                } else {
                    tokens.text_muted
                },
            );
        }
        if toggle_resp.on_hover_text("Развернуть меню").clicked() {
            toggle_sidebar_compact(ui.ctx());
        }
    } else {
        let emblem_rect = Rect::from_min_size(
            Pos2::new(header_rect.left() + 4.0, header_rect.center().y - 16.0),
            Vec2::splat(32.0),
        );
        if ui.is_rect_visible(emblem_rect) {
            paint_mq_emblem(ui.painter(), emblem_rect, tokens);
        }

        let toggle_rect = Rect::from_min_size(
            Pos2::new(header_rect.right() - 28.0, header_rect.center().y - 14.0),
            Vec2::splat(28.0),
        );
        let toggle_resp = ui.interact(
            toggle_rect,
            Id::new("mq_sidebar_collapse_toggle"),
            Sense::click(),
        );
        if ui.is_rect_visible(toggle_rect) {
            if toggle_resp.hovered() {
                ui.painter().rect_filled(
                    toggle_rect,
                    CornerRadius::same(NAV_ROW_RADIUS),
                    tokens.bg_surface_hover,
                );
            }
            paint_vector_icon(
                ui.painter(),
                Rect::from_center_size(toggle_rect.center(), Vec2::splat(16.0)),
                VectorIcon::ChevronsLeft,
                if toggle_resp.hovered() {
                    tokens.text
                } else {
                    tokens.text_muted
                },
            );
        }
        if toggle_resp.on_hover_text("Свернуть меню").clicked() {
            toggle_sidebar_compact(ui.ctx());
        }

        let text_left = emblem_rect.right() + 10.0;
        let title_font = FontId::proportional(15.0);
        let title_galley = ui
            .painter()
            .layout_no_wrap("MQ".to_owned(), title_font, tokens.text);
        let version_font = FontId::proportional(11.0);
        let version_galley = ui.painter().layout_no_wrap(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            version_font,
            tokens.text_muted,
        );
        let total_h = title_galley.size().y + 1.0 + version_galley.size().y;
        let top_y = header_rect.center().y - total_h * 0.5;
        ui.painter()
            .galley(Pos2::new(text_left, top_y), title_galley, tokens.text);
        ui.painter().galley(
            Pos2::new(text_left, top_y + 17.0),
            version_galley,
            tokens.text_muted,
        );
    }
}

fn render_bottom_account_profile(
    ui: &mut Ui,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
    is_compact: bool,
    update_available: bool,
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

    let (username, avatar_opt, source_str) = match active_profile {
        Some(prof) => (
            prof.username.as_str(),
            prof.avatar_head.as_ref(),
            account_source_label(prof.kind),
        ),
        None => ("Гость", None, "Офлайн"),
    };

    if update_available {
        let row_w = ui.available_width();
        let (upd_rect, upd_resp) = ui.allocate_exact_size(Vec2::new(row_w, 22.0), Sense::click());
        if ui.is_rect_visible(upd_rect) {
            let painter = ui.painter();
            if upd_resp.hovered() {
                painter.rect_filled(
                    upd_rect,
                    CornerRadius::same(NAV_ROW_RADIUS),
                    tokens.bg_surface_hover,
                );
            }
            if is_compact {
                painter.circle_filled(upd_rect.center(), 3.5, tokens.text_muted);
            } else {
                let dot_c = Pos2::new(upd_rect.left() + 10.0, upd_rect.center().y);
                painter.circle_filled(dot_c, 3.0, tokens.text_muted);
                let font = FontId::proportional(11.0);
                let g = painter.layout_no_wrap(
                    "Доступно обновление".to_owned(),
                    font,
                    tokens.text_muted,
                );
                painter.galley(
                    Pos2::new(dot_c.x + 8.0, upd_rect.center().y - g.size().y * 0.5),
                    g,
                    tokens.text_muted,
                );
            }
        }
        let upd_resp = if is_compact {
            upd_resp.on_hover_text("Доступно обновление")
        } else {
            upd_resp
        };
        if upd_resp.clicked() {
            action = Some(SidebarAction::SelectTab(NavTab::Settings));
        }
        ui.add_space(4.0);
    }

    let popup_id = Id::new("mq_sidebar_quick_account_popup");
    let available_w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(available_w, 44.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        let painter = ui.painter();

        if hovered {
            painter.rect_filled(rect, RADIUS_BUTTON, tokens.bg_surface_hover);
        }

        let fallback;
        let resolved = match avatar_opt {
            Some(av) if av.rgba.len() == mq_auth::SkinHeadAvatar::BYTE_LEN => av,
            _ => {
                fallback = mq_auth::generate_offline_geometric_avatar(username);
                &fallback
            }
        };

        if is_compact {
            let avatar_rect =
                Rect::from_center_size(rect.center(), Vec2::splat(ACCOUNT_AVATAR_SIZE));
            crate::widgets::paint_skin_head_in_rect(painter, avatar_rect, resolved);
        } else {
            let avatar_rect = Rect::from_min_size(
                Pos2::new(
                    rect.left() + 6.0,
                    rect.center().y - ACCOUNT_AVATAR_SIZE * 0.5,
                ),
                Vec2::splat(ACCOUNT_AVATAR_SIZE),
            );
            crate::widgets::paint_skin_head_in_rect(painter, avatar_rect, resolved);

            let chev_rect = Rect::from_center_size(
                Pos2::new(rect.right() - 14.0, rect.center().y),
                Vec2::splat(16.0),
            );
            paint_vector_icon(
                painter,
                chev_rect,
                VectorIcon::ChevronDown,
                tokens.text_muted,
            );

            let text_left = avatar_rect.right() + 8.0;
            let max_text_w = (chev_rect.left() - 4.0 - text_left).max(20.0);

            let name_font = FontId::proportional(12.5);
            let name_galley =
                truncate_text_to_width(painter, username, &name_font, max_text_w, tokens.text);
            let name_pos = Pos2::new(text_left, rect.top() + 7.0);
            painter.galley(name_pos, name_galley, tokens.text);

            let kind_font = FontId::proportional(11.0);
            let kind_galley =
                painter.layout_no_wrap(source_str.to_owned(), kind_font, tokens.text_muted);
            let kind_pos = Pos2::new(text_left, rect.top() + 23.0);
            painter.galley(kind_pos, kind_galley, tokens.text_muted);
        }
    }

    let resp = if is_compact {
        resp.on_hover_text(format!("{username} ({source_str})"))
    } else {
        resp
    };

    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    egui::popup_above_or_below_widget(
        ui,
        popup_id,
        &resp,
        egui::AboveOrBelow::Above,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(200.0);
            ui.label(
                RichText::new("Аккаунты")
                    .color(tokens.text_muted)
                    .size(11.0),
            );
            ui.separator();
            if accounts.is_empty() {
                ui.label(RichText::new("Гость (Офлайн)").color(tokens.text_muted));
            } else {
                for acc in accounts {
                    let is_current = active_profile
                        .map(|curr| curr.id == acc.id)
                        .unwrap_or(false);
                    let label = format!("{} ({})", acc.username, account_source_label(acc.kind));
                    if ui
                        .selectable_label(is_current, RichText::new(label).color(tokens.text))
                        .clicked()
                    {
                        action = Some(SidebarAction::SwitchAccount(acc.id.to_string()));
                        ui.memory_mut(|mem| mem.close_popup());
                    }
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

    action
}

/// Renders the sidebar contents inside an existing [`egui::Ui`] with default [`SidebarState`].
pub fn render_sidebar(
    ui: &mut Ui,
    active_tab: NavTab,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
    lang_code: &str,
    tokens: &Tokens,
) -> Option<SidebarAction> {
    render_sidebar_with_state(
        ui,
        active_tab,
        accounts,
        active_account_id,
        lang_code,
        tokens,
        SidebarState::default(),
    )
}

/// Renders the sidebar contents inside an existing [`egui::Ui`] with dynamic badges and update state.
pub fn render_sidebar_with_state(
    ui: &mut Ui,
    active_tab: NavTab,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
    lang_code: &str,
    tokens: &Tokens,
    state: SidebarState,
) -> Option<SidebarAction> {
    let mut action = handle_keyboard_navigation(ui).map(SidebarAction::SelectTab);
    let is_compact = ui.available_width() < 120.0;

    // 1. Brand Header (56 px)
    render_header(ui, is_compact, tokens);

    // 2. Top groups: Библиотека & Активность
    for group in [NavGroup::Library, NavGroup::Activity] {
        ui.add_space(6.0);
        paint_horizontal_divider(ui, tokens.border);
        render_group_heading(ui, group.label(), is_compact, tokens);
        if is_compact {
            ui.add_space(6.0);
        }
        for &tab in group.tabs() {
            let badge = if tab == NavTab::Downloads {
                Some(state.active_downloads)
            } else {
                None
            };
            if render_nav_row(
                ui,
                tab,
                tab == active_tab,
                is_compact,
                badge,
                lang_code,
                tokens,
            ) {
                action = Some(SidebarAction::SelectTab(tab));
            }
        }
    }

    // 3. Bottom-pinned «Система» group + Account block
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if let Some(card_action) = render_bottom_account_profile(
            ui,
            accounts,
            active_account_id,
            is_compact,
            state.update_available,
            tokens,
        ) {
            action = Some(card_action);
        }
        ui.add_space(6.0);
        paint_horizontal_divider(ui, tokens.border);
        ui.add_space(6.0);

        for &tab in NavGroup::System.tabs().iter().rev() {
            if render_nav_row(
                ui,
                tab,
                tab == active_tab,
                is_compact,
                None,
                lang_code,
                tokens,
            ) {
                action = Some(SidebarAction::SelectTab(tab));
            }
        }
        if !is_compact {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(NAV_ROW_PADDING_X);
                ui.label(
                    RichText::new(NavGroup::System.label())
                        .size(11.0)
                        .color(tokens.text_muted),
                );
            });
            ui.add_space(8.0);
        } else {
            ui.add_space(6.0);
        }
        paint_horizontal_divider(ui, tokens.border);
    });

    action
}

/// Renders the left [`SidePanel`] and returns any triggered [`SidebarAction`].
pub fn show_sidebar(
    ctx: &Context,
    active_tab: NavTab,
    accounts: &[AccountProfile],
    active_account_id: Option<&str>,
    lang_code: &str,
    tokens: &Tokens,
) -> Option<SidebarAction> {
    let width = sidebar_width(ctx);
    SidePanel::left("mq_left_sidebar")
        .resizable(false)
        .exact_width(width)
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
    use egui::{Event, Modifiers, RawInput};

    #[test]
    fn sidebar_dimensions_and_groups_match_spec() {
        assert_eq!(SIDEBAR_WIDTH, 224.0);
        assert_eq!(COMPACT_SIDEBAR_WIDTH, 64.0);
        assert_eq!(AUTO_COLLAPSE_WINDOW_WIDTH, 1000.0);
        assert_eq!(SIDEBAR_HEADER_HEIGHT, 56.0);
        assert_eq!(NAV_ROW_HEIGHT, 36.0);
        assert_eq!(NAV_ROW_PADDING_X, 12.0);
        assert_eq!(NAV_ICON_SIZE, 16.0);
        assert_eq!(NAV_ICON_TEXT_GAP, 12.0);
        assert_eq!(NAV_ROW_RADIUS, 6);
        assert_eq!(ACCOUNT_AVATAR_SIZE, 28.0);

        assert_eq!(NavGroup::Library.label(), "Библиотека");
        assert_eq!(
            NavGroup::Library.tabs(),
            &[NavTab::Instances, NavTab::Catalog]
        );
        assert_eq!(NavGroup::Activity.label(), "Активность");
        assert_eq!(
            NavGroup::Activity.tabs(),
            &[NavTab::Downloads, NavTab::Performance]
        );
        assert_eq!(NavGroup::System.label(), "Система");
        assert_eq!(
            NavGroup::System.tabs(),
            &[NavTab::Accounts, NavTab::Settings]
        );
        assert_eq!(NavTab::ALL.len(), 6);
    }

    #[test]
    fn auto_collapse_threshold_and_manual_override() {
        // Auto-collapse below 1000 px
        assert!(resolve_compact_mode(999.0, None));
        assert!(resolve_compact_mode(800.0, None));
        assert!(!resolve_compact_mode(1000.0, None));
        assert!(!resolve_compact_mode(1280.0, None));

        // Manual override takes precedence over window width
        assert!(!resolve_compact_mode(800.0, Some(false)));
        assert!(resolve_compact_mode(1280.0, Some(true)));
    }

    #[test]
    fn sidebar_width_responds_to_viewport_and_toggle() {
        let ctx = Context::default();
        let wide_input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            ..Default::default()
        };
        ctx.begin_pass(wide_input);
        assert!(!is_sidebar_compact(&ctx));
        let _ = ctx.end_pass();

        let narrow_input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 800.0))),
            ..Default::default()
        };
        ctx.begin_pass(narrow_input.clone());
        assert!(is_sidebar_compact(&ctx));
        toggle_sidebar_compact(&ctx);
        assert!(!is_sidebar_compact(&ctx));
        let _ = ctx.end_pass();

        // Manual choice persists on subsequent frames
        ctx.begin_pass(narrow_input);
        assert!(!is_sidebar_compact(&ctx));
        let _ = ctx.end_pass();
    }

    #[test]
    fn keyboard_shortcuts_ctrl_1_to_6_select_tabs() {
        let ctx = Context::default();
        let tokens = Tokens::matte_dark();

        for (idx, expected_tab) in NavTab::ALL.iter().enumerate() {
            let key = match idx {
                0 => Key::Num1,
                1 => Key::Num2,
                2 => Key::Num3,
                3 => Key::Num4,
                4 => Key::Num5,
                _ => Key::Num6,
            };
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
                modifiers: Modifiers::CTRL,
                events: vec![Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                }],
                ..Default::default()
            };
            let mut emitted = None;
            ctx.begin_pass(input);
            SidePanel::left("test_sidebar")
                .exact_width(SIDEBAR_WIDTH)
                .show(&ctx, |ui| {
                    emitted = render_sidebar_with_state(
                        ui,
                        NavTab::Instances,
                        &[],
                        None,
                        "ru",
                        &tokens,
                        SidebarState {
                            active_downloads: 3,
                            update_available: true,
                        },
                    );
                });
            let _ = ctx.end_pass();
            assert_eq!(emitted, Some(SidebarAction::SelectTab(*expected_tab)));
        }
    }

    #[test]
    fn arrow_navigation_and_enter_activates_focused_tab() {
        let ctx = Context::default();
        let tokens = Tokens::matte_dark();

        // Frame 1: focus the first nav item
        ctx.begin_pass(RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
            ..Default::default()
        });
        ctx.memory_mut(|m| m.request_focus(nav_item_id(NavTab::Instances)));
        SidePanel::left("test_sidebar")
            .exact_width(SIDEBAR_WIDTH)
            .show(&ctx, |ui| {
                let _ = render_sidebar(ui, NavTab::Instances, &[], None, "ru", &tokens);
            });
        let _ = ctx.end_pass();

        // Frame 2: ArrowDown moves focus from Instances to Catalog
        ctx.begin_pass(RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
            events: vec![Event::Key {
                key: Key::ArrowDown,
                physical_key: Some(Key::ArrowDown),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        });
        SidePanel::left("test_sidebar")
            .exact_width(SIDEBAR_WIDTH)
            .show(&ctx, |ui| {
                let _ = render_sidebar(ui, NavTab::Instances, &[], None, "ru", &tokens);
            });
        let _ = ctx.end_pass();

        // Frame 3: Enter activates the focused item (Catalog)
        let mut emitted = None;
        ctx.begin_pass(RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
            events: vec![Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        });
        SidePanel::left("test_sidebar")
            .exact_width(SIDEBAR_WIDTH)
            .show(&ctx, |ui| {
                emitted = render_sidebar(ui, NavTab::Instances, &[], None, "ru", &tokens);
            });
        let _ = ctx.end_pass();
        assert_eq!(emitted, Some(SidebarAction::SelectTab(NavTab::Catalog)));
    }
}
