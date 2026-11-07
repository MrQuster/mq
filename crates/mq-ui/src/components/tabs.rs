//! Underline tab bar for detail screens (Console, Mods, Resourcepacks, Shaders, Worlds, etc.)
//! Conforms to specification B5: active tab = text + accent 2px underline, hover = subtle bg,
//! non-wrapping horizontal scroll area for narrow windows.

use egui::{CursorIcon, Response, ScrollArea, Sense, Stroke, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

pub fn render_underline_tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    tokens: &Tokens,
    tabs: &[(T, &str)],
    current: &mut T,
) {
    let converted: Vec<(T, &str, Option<VectorIcon>)> =
        tabs.iter().map(|&(t, label)| (t, label, None)).collect();
    underline_tabs(ui, tokens, current, &converted);
}

pub fn underline_tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    tokens: &Tokens,
    current: &mut T,
    tabs: &[(T, &str, Option<VectorIcon>)],
) -> Response {
    let mut main_resp = ui.scope(|_| {}).response;

    ScrollArea::horizontal()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);
                for &(tab, label, opt_icon) in tabs {
                    let is_selected = *current == tab;
                    let text_color = if is_selected {
                        tokens.text
                    } else {
                        tokens.text_muted
                    };

                    let mut width = label.len() as f32 * 8.0 + 16.0;
                    if opt_icon.is_some() {
                        width += 20.0;
                    }
                    width = width.max(48.0);

                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());

                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }

                    if response.clicked() {
                        *current = tab;
                        main_resp.mark_changed();
                    }

                    let painter = ui.painter();

                    if response.hovered() && !is_selected {
                        painter.rect_filled(
                            rect.shrink2(Vec2::new(2.0, 4.0)),
                            RADIUS_BUTTON,
                            tokens.bg_surface_hover.gamma_multiply(0.4),
                        );
                    }

                    let mut text_x = rect.center().x;
                    if let Some(icon) = opt_icon {
                        let icon_rect = egui::Rect::from_center_size(
                            egui::pos2(rect.left() + 14.0, rect.center().y - 1.0),
                            Vec2::splat(14.0),
                        );
                        paint_vector_icon(painter, icon_rect, icon, text_color);
                        text_x = rect.left() + 26.0;
                    }

                    let text_align = if opt_icon.is_some() {
                        egui::Align2::LEFT_CENTER
                    } else {
                        egui::Align2::CENTER_CENTER
                    };

                    painter.text(
                        egui::pos2(text_x, rect.center().y - 2.0),
                        text_align,
                        label,
                        egui::FontId::proportional(13.5),
                        text_color,
                    );

                    if is_selected {
                        painter.line_segment(
                            [
                                egui::pos2(rect.left(), rect.bottom() - 1.0),
                                egui::pos2(rect.right(), rect.bottom() - 1.0),
                            ],
                            Stroke::new(2.5_f32, tokens.accent),
                        );
                    }
                }
            });
        });

    main_resp
}
