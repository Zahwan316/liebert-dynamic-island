use std::f32::consts::PI;

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, Pos2, Rect, Response, Sense, Stroke, Vec2,
};

#[allow(dead_code)] // Shared vector icon variants across views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Settings,
    MusicNote,
    TodoCheck,
    Calendar,
    Shuffle,
    Previous,
    Play,
    Pause,
    Next,
    Repeat,
    RepeatOne,
    Close,
    Dot,
}

pub fn icon_button(
    ui: &mut egui::Ui,
    kind: IconKind,
    hit_size: Vec2,
    icon_size: f32,
    color: Color32,
    fill: Color32,
    corner_radius: CornerRadius,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(hit_size, Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let bg = if resp.is_pointer_button_down_on() {
            Color32::from_rgba_unmultiplied(255, 255, 255, fill.a().saturating_add(28))
        } else if resp.hovered() {
            Color32::from_rgba_unmultiplied(255, 255, 255, fill.a().saturating_add(18))
        } else {
            fill
        };
        if bg.a() > 0 {
            ui.painter().rect_filled(rect, corner_radius, bg);
        }
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
        paint_icon(ui.painter(), icon_rect, kind, color);
    }
    resp
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, kind: IconKind, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke_w = (s * 0.13).clamp(1.2, 2.4);
    let stroke = Stroke::new(stroke_w, color);

    match kind {
        IconKind::Dot => {
            painter.circle_filled(c, s * 0.36, color);
        }
        IconKind::Close => {
            let r = s * 0.28;
            painter.line_segment([Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)], stroke);
            painter.line_segment([Pos2::new(c.x + r, c.y - r), Pos2::new(c.x - r, c.y + r)], stroke);
        }
        IconKind::Settings => {
            let outer_r = s * 0.38;
            let inner_r = s * 0.17;
            let tooth_r = s * 0.46;
            painter.circle_stroke(c, outer_r * 0.78, stroke);
            painter.circle_filled(c, inner_r * 0.55, color);
            for i in 0..8 {
                let ang = (i as f32) * (PI * 0.25);
                let (sin_a, cos_a) = ang.sin_cos();
                let p0 = Pos2::new(c.x + outer_r * 0.78 * cos_a, c.y + outer_r * 0.78 * sin_a);
                let p1 = Pos2::new(c.x + tooth_r * cos_a, c.y + tooth_r * sin_a);
                painter.line_segment([p0, p1], Stroke::new(stroke_w * 1.15, color));
            }
        }
        IconKind::MusicNote => {
            let head = Pos2::new(c.x - s * 0.12, c.y + s * 0.22);
            let stem_top = Pos2::new(head.x + s * 0.14, c.y - s * 0.30);
            let stem_bot = Pos2::new(stem_top.x, head.y);
            painter.circle_filled(head, s * 0.16, color);
            painter.line_segment([stem_bot, stem_top], stroke);
            let flag_end = Pos2::new(stem_top.x + s * 0.22, stem_top.y + s * 0.14);
            painter.line_segment([stem_top, flag_end], Stroke::new(stroke_w * 1.15, color));
        }
        IconKind::TodoCheck => {
            let box_rect = Rect::from_center_size(c, Vec2::splat(s * 0.76));
            painter.rect_stroke(
                box_rect,
                CornerRadius::same((s * 0.18).round() as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            let p1 = Pos2::new(c.x - s * 0.20, c.y + s * 0.02);
            let p2 = Pos2::new(c.x - s * 0.04, c.y + s * 0.18);
            let p3 = Pos2::new(c.x + s * 0.22, c.y - s * 0.16);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
        }
        IconKind::Calendar => {
            let box_rect = Rect::from_center_size(
                Pos2::new(c.x, c.y + s * 0.05),
                Vec2::new(s * 0.76, s * 0.68),
            );
            painter.rect_stroke(
                box_rect,
                CornerRadius::same((s * 0.14).round() as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            let bar_y = box_rect.top() + s * 0.20;
            painter.line_segment(
                [Pos2::new(box_rect.left(), bar_y), Pos2::new(box_rect.right(), bar_y)],
                stroke,
            );
            let pin_top = box_rect.top() - s * 0.10;
            let pin_bot = box_rect.top() + s * 0.05;
            painter.line_segment(
                [Pos2::new(c.x - s * 0.18, pin_top), Pos2::new(c.x - s * 0.18, pin_bot)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(c.x + s * 0.18, pin_top), Pos2::new(c.x + s * 0.18, pin_bot)],
                stroke,
            );
        }
        IconKind::Play => {
            let half_h = s * 0.34;
            let left = c.x - s * 0.22;
            let right = c.x + s * 0.30;
            let pts = vec![
                Pos2::new(left, c.y - half_h),
                Pos2::new(right, c.y),
                Pos2::new(left, c.y + half_h),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        IconKind::Pause => {
            let bar_w = (s * 0.18).max(2.0);
            let bar_h = s * 0.66;
            let gap = s * 0.13;
            let r = CornerRadius::same(1);
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x - gap, c.y), Vec2::new(bar_w, bar_h)),
                r,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + gap, c.y), Vec2::new(bar_w, bar_h)),
                r,
                color,
            );
        }
        IconKind::Previous => {
            let bar_w = (s * 0.13).max(1.8);
            let bar_h = s * 0.56;
            let bar_x = c.x - s * 0.26;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(bar_x, c.y), Vec2::new(bar_w, bar_h)),
                CornerRadius::same(1),
                color,
            );
            let half_h = s * 0.28;
            let tip_x = bar_x + bar_w * 0.4;
            let base_x = c.x + s * 0.28;
            let pts = vec![
                Pos2::new(base_x, c.y - half_h),
                Pos2::new(tip_x, c.y),
                Pos2::new(base_x, c.y + half_h),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        IconKind::Next => {
            let bar_w = (s * 0.13).max(1.8);
            let bar_h = s * 0.56;
            let bar_x = c.x + s * 0.26;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(bar_x, c.y), Vec2::new(bar_w, bar_h)),
                CornerRadius::same(1),
                color,
            );
            let half_h = s * 0.28;
            let base_x = c.x - s * 0.28;
            let tip_x = bar_x - bar_w * 0.4;
            let pts = vec![
                Pos2::new(base_x, c.y - half_h),
                Pos2::new(tip_x, c.y),
                Pos2::new(base_x, c.y + half_h),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        IconKind::Shuffle => {
            let lx = c.x - s * 0.34;
            let rx = c.x + s * 0.32;
            let ty = c.y - s * 0.20;
            let by = c.y + s * 0.20;
            painter.line_segment([Pos2::new(lx, by), Pos2::new(rx, ty)], stroke);
            painter.line_segment([Pos2::new(lx, ty), Pos2::new(rx, by)], stroke);
            let ah = s * 0.13;
            painter.line_segment([Pos2::new(rx - ah, ty - ah * 0.2), Pos2::new(rx, ty)], stroke);
            painter.line_segment([Pos2::new(rx - ah * 0.2, ty + ah), Pos2::new(rx, ty)], stroke);
            painter.line_segment([Pos2::new(rx - ah * 0.2, by - ah), Pos2::new(rx, by)], stroke);
            painter.line_segment([Pos2::new(rx - ah, by + ah * 0.2), Pos2::new(rx, by)], stroke);
        }
        IconKind::Repeat | IconKind::RepeatOne => {
            let lx = c.x - s * 0.32;
            let rx = c.x + s * 0.32;
            let ty = c.y - s * 0.20;
            let by = c.y + s * 0.20;
            painter.line_segment([Pos2::new(lx, ty), Pos2::new(rx, ty)], stroke);
            painter.line_segment([Pos2::new(lx, ty), Pos2::new(lx, c.y - s * 0.02)], stroke);
            painter.line_segment([Pos2::new(rx, by), Pos2::new(lx, by)], stroke);
            painter.line_segment([Pos2::new(rx, by), Pos2::new(rx, c.y + s * 0.02)], stroke);
            let ah = s * 0.13;
            painter.line_segment([Pos2::new(rx - ah, ty - ah), Pos2::new(rx, ty)], stroke);
            painter.line_segment([Pos2::new(rx - ah, ty + ah), Pos2::new(rx, ty)], stroke);
            painter.line_segment([Pos2::new(lx + ah, by - ah), Pos2::new(lx, by)], stroke);
            painter.line_segment([Pos2::new(lx + ah, by + ah), Pos2::new(lx, by)], stroke);
            if kind == IconKind::RepeatOne {
                painter.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    "1",
                    egui::FontId::proportional((s * 0.42).max(8.0)),
                    color,
                );
            }
        }
    }
}
