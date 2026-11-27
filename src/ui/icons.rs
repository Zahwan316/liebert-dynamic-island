use std::f32::consts::{FRAC_PI_2, PI};

use egui::{
    self, Color32, CornerRadius, CursorIcon, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2,
};

pub const SETTINGS_WINDOW_TITLE: &str = "Liebert Dynamic Island — Settings";

#[allow(dead_code)] // Subset used by overlay; full enum used by Settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Lyrics,
    Features,
    System,
    Battery,
    Todo,
    Calendar,
    About,
}

#[allow(dead_code)] // Navigation metadata used by Settings sidebar & shortcuts.
impl SettingsTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Display",
            Self::Lyrics => "Lyrics",
            Self::Features => "Features",
            Self::System => "System",
            Self::Battery => "Battery",
            Self::Todo => "Tasks",
            Self::Calendar => "Calendar",
            Self::About => "About",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Appearance => "Island surface, glass material, mascot, clock & typography",
            Self::Lyrics => "Media sync, karaoke highlight, romaji & translation",
            Self::Features => "Auto-hide, fullscreen rules, notifications & Pomodoro",
            Self::System => "OS startup & taskbar system monitor layout",
            Self::Battery => "Device power telemetry & low-battery alerts",
            Self::Todo => "Local quick checklist stored in todos.toml",
            Self::Calendar => "Google Calendar secret .ics sync & upcoming schedule",
            Self::About => "Creator, application version, license & runtime info",
        }
    }

    pub fn icon(self) -> IconKind {
        match self {
            Self::Appearance => IconKind::Settings,
            Self::Lyrics => IconKind::MusicNote,
            Self::Features => IconKind::Timer,
            Self::System => IconKind::Cpu,
            Self::Battery => IconKind::Dot,
            Self::Todo => IconKind::TodoCheck,
            Self::Calendar => IconKind::Calendar,
            Self::About => IconKind::Info,
        }
    }

    pub fn all() -> [Self; 8] {
        [
            Self::Appearance,
            Self::Lyrics,
            Self::Features,
            Self::System,
            Self::Battery,
            Self::Todo,
            Self::Calendar,
            Self::About,
        ]
    }
}

#[allow(dead_code)] // Shared vector icon variants across views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Settings,
    MusicNote,
    TodoCheck,
    Calendar,
    Timer,
    Bell,
    Cpu,
    Gpu,
    Thermometer,
    Memory,
    Disk,
    ArrowUp,
    ArrowDown,
    Info,
    Shuffle,
    Previous,
    Play,
    Pause,
    Next,
    Repeat,
    RepeatOne,
    Close,
    Dot,
    ChevronLeft,
    ChevronRight,
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

#[derive(Debug, Clone, Copy, Default)]
pub struct MascotMusicMotion {
    pub bars: [f32; 3],
    pub dot: f32,
    /// Normalized neck-cock tilt angle in `-1.0..=1.0` (`±19°` around the neck pivot).
    pub nod_x: f32,
    /// Forward head nod pitch in `0.0..=1.0` simulating 3D `rotateX` beat foreshortening.
    pub pitch_x: f32,
    pub yepii: f32,
}

fn transform_mascot_pt(
    pt: Pos2,
    pivot: Pos2,
    sin_a: f32,
    cos_a: f32,
    scale_y: f32,
    nod_drop: f32,
) -> Pos2 {
    let dx = pt.x - pivot.x;
    let dy = (pt.y - pivot.y) * scale_y + nod_drop;
    Pos2::new(
        pivot.x + dx * cos_a - dy * sin_a,
        pivot.y + dx * sin_a + dy * cos_a,
    )
}

fn rounded_rect_points(rect: Rect, cr: CornerRadius) -> Vec<Pos2> {
    let hw = rect.width() * 0.5;
    let hh = rect.height() * 0.5;
    let max_r = hw.min(hh).max(0.0);
    let r_nw = (cr.nw as f32).min(max_r);
    let r_ne = (cr.ne as f32).min(max_r);
    let r_se = (cr.se as f32).min(max_r);
    let r_sw = (cr.sw as f32).min(max_r);

    let mut pts = Vec::with_capacity(16);
    let corners = [
        (
            Pos2::new(rect.left() + r_nw, rect.top() + r_nw),
            r_nw,
            PI,
            PI + FRAC_PI_2,
        ),
        (
            Pos2::new(rect.right() - r_ne, rect.top() + r_ne),
            r_ne,
            PI + FRAC_PI_2,
            2.0 * PI,
        ),
        (
            Pos2::new(rect.right() - r_se, rect.bottom() - r_se),
            r_se,
            0.0,
            FRAC_PI_2,
        ),
        (
            Pos2::new(rect.left() + r_sw, rect.bottom() - r_sw),
            r_sw,
            FRAC_PI_2,
            PI,
        ),
    ];
    for (center, r, a0, a1) in corners {
        if r <= 0.25 {
            pts.push(center);
        } else {
            for step in 0..=3 {
                let t = step as f32 / 3.0;
                let a = a0 + (a1 - a0) * t;
                pts.push(Pos2::new(center.x + r * a.cos(), center.y + r * a.sin()));
            }
        }
    }
    pts
}

#[allow(clippy::too_many_arguments)]
fn paint_mascot_rect(
    painter: &egui::Painter,
    rect: Rect,
    cr: CornerRadius,
    color: Color32,
    pivot: Pos2,
    tilt_rad: f32,
    sin_a: f32,
    cos_a: f32,
    scale_y: f32,
    nod_drop: f32,
) {
    if rect.width() <= 0.1 || rect.height() <= 0.1 {
        return;
    }
    if tilt_rad.abs() < 0.002 && (scale_y - 1.0).abs() < 0.002 && nod_drop.abs() < 0.05 {
        painter.rect_filled(rect, cr, color);
        return;
    }
    let pts: Vec<Pos2> = rounded_rect_points(rect, cr)
        .into_iter()
        .map(|p| transform_mascot_pt(p, pivot, sin_a, cos_a, scale_y, nod_drop))
        .collect();
    painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn paint_mascot(painter: &egui::Painter, rect: Rect, eye_offset_norm: Vec2, color: Color32) {
    paint_mascot_fx(painter, rect, eye_offset_norm, 1.0, Some(color), color);
}

/// Renders the mascot with eye blink aperture (`eye_open` in `0.14..=1.0`) and optional aura glow.
pub fn paint_mascot_fx(
    painter: &egui::Painter,
    rect: Rect,
    eye_offset_norm: Vec2,
    eye_open: f32,
    glow_color: Option<Color32>,
    color: Color32,
) {
    paint_mascot_inner(
        painter,
        rect,
        eye_offset_norm,
        eye_open,
        glow_color,
        color,
        None,
    );
}

/// Renders the dancing music mascot: mouth morphs into 3 animated equalizer bars + beat capsule,
/// head cocks sharply left/right (~±19°) with beat pitch around a neck pivot like a neck-snap nod
/// plus periodic "yepii" hops, while preserving cursor eye-tracking and natural blinking.
pub fn paint_mascot_music(
    painter: &egui::Painter,
    rect: Rect,
    eye_offset_norm: Vec2,
    eye_open: f32,
    glow_color: Option<Color32>,
    color: Color32,
    motion: MascotMusicMotion,
) {
    paint_mascot_inner(
        painter,
        rect,
        eye_offset_norm,
        eye_open,
        glow_color,
        color,
        Some(motion),
    );
}

fn paint_mascot_inner(
    painter: &egui::Painter,
    rect: Rect,
    eye_offset_norm: Vec2,
    eye_open: f32,
    glow_color: Option<Color32>,
    color: Color32,
    music: Option<MascotMusicMotion>,
) {
    let yepii = music.map_or(0.0, |m| m.yepii.clamp(0.0, 1.0));
    let nod_x = music.map_or(0.0, |m| m.nod_x.clamp(-1.0, 1.0));
    let pitch_x = music.map_or(0.0, |m| m.pitch_x.clamp(0.0, 1.0));
    let scale_boost = 1.0 + 0.14 * yepii;
    let h = rect.height().min(rect.width()).max(8.0) * scale_boost;
    let u = h / 24.0;
    let c = rect.center() + Vec2::new(nod_x * rect.width() * 0.03, -yepii * rect.height() * 0.08);
    // Anchor rotation at the neck/jaw hinge below the mouth so head tilts read like a crisp neck snap.
    let pivot = Pos2::new(c.x, c.y + 10.5 * u);
    let tilt_rad = nod_x * 19.0_f32.to_radians();
    let (sin_a, cos_a) = tilt_rad.sin_cos();
    let scale_y = 1.0 - 0.16 * pitch_x;
    let nod_drop = pitch_x * 1.45 * u;

    let offset_len = eye_offset_norm.length();
    let norm = if offset_len > 1.0 {
        eye_offset_norm / offset_len
    } else {
        eye_offset_norm
    };
    let eye_dx = norm.x * 2.8 * u;
    let eye_dy = norm.y * 2.0 * u - yepii * 0.55 * u;
    let mouth_dx = norm.x * 2.8 * u * 0.14;
    let mouth_dy = norm.y * 2.0 * u * 0.14 + yepii * 0.20 * u;

    let open = eye_open.clamp(0.14, 1.0);
    let base_eye_h = 10.2 * u;
    let eye_t = (3.0 * u).max(1.8);
    let eye_h = (base_eye_h * open).max(eye_t * 0.85);
    let eye_cap = (eye_t * 0.5).round().clamp(1.0, 16.0) as u8;
    let l_width = 6.6 * u;
    let eye_gap = 4.4 * u;
    let total_eyes_w = l_width + eye_gap + eye_t;

    let eyes_left = c.x - total_eyes_w * 0.5 + eye_dx;
    let base_eyes_cy = c.y - 9.8 * u + base_eye_h * 0.5 + eye_dy;
    let eye_top = base_eyes_cy - eye_h * 0.5;

    let paint_glow_rect = |r: Rect, cr: CornerRadius| {
        let Some(gc) = glow_color else {
            return;
        };
        let outer = (2.2 * u).max(1.5);
        let inner = (1.0 * u).max(0.8);
        paint_mascot_rect(
            painter,
            r.expand(outer),
            cr,
            Color32::from_rgba_unmultiplied(gc.r(), gc.g(), gc.b(), (gc.a() as f32 * 0.12) as u8),
            pivot,
            tilt_rad,
            sin_a,
            cos_a,
            scale_y,
            nod_drop,
        );
        paint_mascot_rect(
            painter,
            r.expand(inner),
            cr,
            Color32::from_rgba_unmultiplied(gc.r(), gc.g(), gc.b(), (gc.a() as f32 * 0.26) as u8),
            pivot,
            tilt_rad,
            sin_a,
            cos_a,
            scale_y,
            nod_drop,
        );
    };

    let stem_h = (eye_h - eye_t).max(0.0);
    let stem_rect = Rect::from_min_max(
        Pos2::new(eyes_left, eye_top),
        Pos2::new(eyes_left + eye_t, eye_top + stem_h),
    );
    let foot_y0 = eye_top + stem_h;
    let foot_rect = Rect::from_min_max(
        Pos2::new(eyes_left, foot_y0),
        Pos2::new(
            eyes_left + l_width,
            (eye_top + eye_h).max(foot_y0 + eye_t * 0.85),
        ),
    );
    let right_eye_x = eyes_left + l_width + eye_gap;
    let right_eye_rect = Rect::from_min_max(
        Pos2::new(right_eye_x, eye_top),
        Pos2::new(right_eye_x + eye_t, eye_top + eye_h),
    );

    let mouth_h = (4.8 * u).max(3.2);
    let mouth_pill_w = 10.5 * u;
    let mouth_gap = 1.6 * u;
    let mouth_dot_w = (2.2 * u).max(1.5);
    let total_mouth_w = mouth_pill_w + mouth_gap + mouth_dot_w;
    let mouth_left = c.x - total_mouth_w * 0.5 + mouth_dx;
    let mouth_top = c.y + 4.4 * u + mouth_dy;
    let mouth_pill_rect = Rect::from_min_size(
        Pos2::new(mouth_left, mouth_top),
        Vec2::new(mouth_pill_w, mouth_h),
    );
    let mouth_stroke = (1.45 * u).clamp(1.0, 2.8);
    let mouth_cap = (mouth_h * 0.5).round().clamp(1.0, 16.0) as u8;
    let mouth_dot_rect = Rect::from_min_size(
        Pos2::new(mouth_left + mouth_pill_w + mouth_gap, mouth_top),
        Vec2::new(mouth_dot_w, mouth_h),
    );
    let dot_cap = (mouth_dot_w * 0.5).round().clamp(1.0, 12.0) as u8;

    // Soft luminous aura beneath mascot shapes.
    if stem_h > 0.5 {
        paint_glow_rect(stem_rect, CornerRadius::same(eye_cap));
    }
    paint_glow_rect(foot_rect, CornerRadius::same(eye_cap));
    paint_glow_rect(right_eye_rect, CornerRadius::same(eye_cap));

    if let Some(m) = music {
        // Music mode: 3 animated vertical equalizer pills replace the oval mouth,
        // accompanied by the signature vertical capsule on the right.
        let max_bar_h = (7.2 * u).max(4.4);
        let min_bar_h = (2.6 * u).max(2.0);
        let eq_bar_w = ((mouth_pill_w - 2.4 * u) / 3.0).clamp(1.6, 3.2 * u);
        let eq_gap = ((mouth_pill_w - eq_bar_w * 3.0) * 0.5).max(1.1 * u);
        let eq_cy = mouth_pill_rect.center().y;
        let eq_cap = (eq_bar_w * 0.5).round().clamp(1.0, 12.0) as u8;
        let eq_bars: [Rect; 3] = std::array::from_fn(|i| {
            let frac = m.bars[i].clamp(0.18, 1.0);
            let bh = min_bar_h + (max_bar_h - min_bar_h) * frac;
            let bx = mouth_left + i as f32 * (eq_bar_w + eq_gap);
            Rect::from_center_size(
                Pos2::new(bx + eq_bar_w * 0.5, eq_cy),
                Vec2::new(eq_bar_w, bh),
            )
        });
        let right_dot_h = min_bar_h + (max_bar_h * 0.86 - min_bar_h) * m.dot.clamp(0.25, 1.0);
        let right_bar = Rect::from_center_size(
            Pos2::new(
                mouth_left + mouth_pill_w + mouth_gap + mouth_dot_w * 0.5,
                eq_cy,
            ),
            Vec2::new(mouth_dot_w.max(eq_bar_w * 0.88), right_dot_h),
        );
        let right_cap = (right_bar.width() * 0.5).round().clamp(1.0, 12.0) as u8;
        for r in eq_bars {
            paint_glow_rect(r, CornerRadius::same(eq_cap));
        }
        paint_glow_rect(right_bar, CornerRadius::same(right_cap));
        for r in eq_bars {
            paint_mascot_rect(
                painter,
                r,
                CornerRadius::same(eq_cap),
                color,
                pivot,
                tilt_rad,
                sin_a,
                cos_a,
                scale_y,
                nod_drop,
            );
        }
        paint_mascot_rect(
            painter,
            right_bar,
            CornerRadius::same(right_cap),
            color,
            pivot,
            tilt_rad,
            sin_a,
            cos_a,
            scale_y,
            nod_drop,
        );
    } else {
        if let Some(gc) = glow_color {
            painter.rect_stroke(
                mouth_pill_rect.expand(1.2 * u),
                CornerRadius::same(mouth_cap.saturating_add(1)),
                Stroke::new(
                    mouth_stroke + 1.8 * u,
                    Color32::from_rgba_unmultiplied(
                        gc.r(),
                        gc.g(),
                        gc.b(),
                        (gc.a() as f32 * 0.16) as u8,
                    ),
                ),
                egui::StrokeKind::Middle,
            );
        }
        paint_glow_rect(mouth_dot_rect, CornerRadius::same(dot_cap));
    }

    // Left eye ("L"): crisp rounded stem + foot (collapses smoothly during blink).
    if stem_h > 0.5 {
        paint_mascot_rect(
            painter,
            stem_rect,
            CornerRadius {
                nw: eye_cap,
                ne: eye_cap,
                sw: 0,
                se: 0,
            },
            color,
            pivot,
            tilt_rad,
            sin_a,
            cos_a,
            scale_y,
            nod_drop,
        );
        paint_mascot_rect(
            painter,
            foot_rect,
            CornerRadius {
                nw: 0,
                ne: eye_cap,
                sw: eye_cap,
                se: eye_cap,
            },
            color,
            pivot,
            tilt_rad,
            sin_a,
            cos_a,
            scale_y,
            nod_drop,
        );
    } else {
        paint_mascot_rect(
            painter,
            foot_rect,
            CornerRadius::same(eye_cap),
            color,
            pivot,
            tilt_rad,
            sin_a,
            cos_a,
            scale_y,
            nod_drop,
        );
    }

    // Right eye ("I"): vertical rounded capsule.
    paint_mascot_rect(
        painter,
        right_eye_rect,
        CornerRadius::same(eye_cap),
        color,
        pivot,
        tilt_rad,
        sin_a,
        cos_a,
        scale_y,
        nod_drop,
    );

    if music.is_none() {
        // Idle mouth: hollow horizontal pill + solid vertical pill on the right.
        painter.rect_stroke(
            mouth_pill_rect,
            CornerRadius::same(mouth_cap),
            Stroke::new(mouth_stroke, color),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(mouth_dot_rect, CornerRadius::same(dot_cap), color);
    }
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
        IconKind::ChevronLeft => {
            let tip = Pos2::new(c.x - s * 0.17, c.y);
            let top = Pos2::new(c.x + s * 0.15, c.y - s * 0.26);
            let bot = Pos2::new(c.x + s * 0.15, c.y + s * 0.26);
            painter.line_segment([top, tip], stroke);
            painter.line_segment([tip, bot], stroke);
        }
        IconKind::ChevronRight => {
            let tip = Pos2::new(c.x + s * 0.17, c.y);
            let top = Pos2::new(c.x - s * 0.15, c.y - s * 0.26);
            let bot = Pos2::new(c.x - s * 0.15, c.y + s * 0.26);
            painter.line_segment([top, tip], stroke);
            painter.line_segment([tip, bot], stroke);
        }
        IconKind::Close => {
            let r = s * 0.28;
            painter.line_segment(
                [Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(c.x + r, c.y - r), Pos2::new(c.x - r, c.y + r)],
                stroke,
            );
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
                [
                    Pos2::new(box_rect.left(), bar_y),
                    Pos2::new(box_rect.right(), bar_y),
                ],
                stroke,
            );
            let pin_top = box_rect.top() - s * 0.10;
            let pin_bot = box_rect.top() + s * 0.05;
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.18, pin_top),
                    Pos2::new(c.x - s * 0.18, pin_bot),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.18, pin_top),
                    Pos2::new(c.x + s * 0.18, pin_bot),
                ],
                stroke,
            );
        }
        IconKind::Timer => {
            painter.circle_stroke(c, s * 0.36, stroke);
            painter.line_segment([c, Pos2::new(c.x, c.y - s * 0.22)], stroke);
            painter.line_segment([c, Pos2::new(c.x + s * 0.16, c.y + s * 0.08)], stroke);
        }
        IconKind::Bell => {
            let dome = Rect::from_center_size(
                Pos2::new(c.x, c.y - s * 0.04),
                Vec2::new(s * 0.56, s * 0.58),
            );
            painter.rect_stroke(
                dome,
                CornerRadius {
                    nw: (s * 0.26).round() as u8,
                    ne: (s * 0.26).round() as u8,
                    sw: 2,
                    se: 2,
                },
                stroke,
                egui::StrokeKind::Inside,
            );
            let brim_y = dome.bottom() + s * 0.03;
            painter.line_segment(
                [
                    Pos2::new(c.x - s * 0.36, brim_y),
                    Pos2::new(c.x + s * 0.36, brim_y),
                ],
                stroke,
            );
            painter.circle_filled(
                Pos2::new(c.x, brim_y + s * 0.13),
                (s * 0.09).max(1.3),
                color,
            );
        }
        IconKind::Cpu => {
            let body = Rect::from_center_size(c, Vec2::splat(s * 0.52));
            painter.rect_stroke(
                body,
                CornerRadius::same((s * 0.10).round() as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::splat(s * 0.20)),
                CornerRadius::same(1),
                color,
            );
            let p_len = s * 0.13;
            for delta in [-s * 0.12, s * 0.12] {
                painter.line_segment(
                    [
                        Pos2::new(c.x + delta, body.top()),
                        Pos2::new(c.x + delta, body.top() - p_len),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(c.x + delta, body.bottom()),
                        Pos2::new(c.x + delta, body.bottom() + p_len),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(body.left(), c.y + delta),
                        Pos2::new(body.left() - p_len, c.y + delta),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(body.right(), c.y + delta),
                        Pos2::new(body.right() + p_len, c.y + delta),
                    ],
                    stroke,
                );
            }
        }
        IconKind::Gpu => {
            let board = Rect::from_center_size(c, Vec2::new(s * 0.78, s * 0.52));
            painter.rect_stroke(
                board,
                CornerRadius::same((s * 0.10).round() as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_stroke(
                Pos2::new(c.x - s * 0.16, c.y),
                s * 0.14,
                Stroke::new((stroke_w * 0.9).max(1.0), color),
            );
            painter.circle_stroke(
                Pos2::new(c.x + s * 0.16, c.y),
                s * 0.14,
                Stroke::new((stroke_w * 0.9).max(1.0), color),
            );
            painter.line_segment(
                [
                    Pos2::new(board.left() - s * 0.08, board.top()),
                    Pos2::new(board.left() - s * 0.08, board.bottom() + s * 0.12),
                ],
                stroke,
            );
        }
        IconKind::Thermometer => {
            let bulb = Pos2::new(c.x, c.y + s * 0.24);
            painter.circle_filled(bulb, s * 0.15, color);
            let stem = Rect::from_min_max(
                Pos2::new(c.x - s * 0.09, c.y - s * 0.36),
                Pos2::new(c.x + s * 0.09, bulb.y),
            );
            painter.rect_stroke(
                stem,
                CornerRadius::same((s * 0.09).round().max(1.0) as u8),
                Stroke::new((stroke_w * 0.9).max(1.0), color),
                egui::StrokeKind::Outside,
            );
            painter.line_segment(
                [Pos2::new(c.x, c.y - s * 0.14), bulb],
                Stroke::new((stroke_w * 0.9).max(1.0), color),
            );
        }
        IconKind::Memory => {
            let stick = Rect::from_center_size(c, Vec2::new(s * 0.78, s * 0.44));
            painter.rect_stroke(
                stick,
                CornerRadius::same((s * 0.08).round() as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            for dx in [-s * 0.20, 0.0, s * 0.20] {
                painter.rect_filled(
                    Rect::from_center_size(
                        Pos2::new(c.x + dx, c.y - s * 0.02),
                        Vec2::new(s * 0.11, s * 0.20),
                    ),
                    CornerRadius::same(1),
                    color,
                );
                painter.line_segment(
                    [
                        Pos2::new(c.x + dx, stick.bottom()),
                        Pos2::new(c.x + dx, stick.bottom() + s * 0.12),
                    ],
                    stroke,
                );
            }
        }
        IconKind::Disk => {
            let drive = Rect::from_center_size(c, Vec2::new(s * 0.68, s * 0.72));
            painter.rect_stroke(
                drive,
                CornerRadius::same((s * 0.14).round() as u8),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_stroke(
                Pos2::new(c.x, c.y - s * 0.08),
                s * 0.16,
                Stroke::new((stroke_w * 0.85).max(1.0), color),
            );
            painter.circle_filled(
                Pos2::new(c.x + s * 0.16, drive.bottom() - s * 0.13),
                (s * 0.06).max(1.1),
                color,
            );
        }
        IconKind::ArrowUp => {
            let top = Pos2::new(c.x, c.y - s * 0.30);
            let bot = Pos2::new(c.x, c.y + s * 0.30);
            painter.line_segment([bot, top], stroke);
            painter.line_segment([top, Pos2::new(c.x - s * 0.20, c.y - s * 0.08)], stroke);
            painter.line_segment([top, Pos2::new(c.x + s * 0.20, c.y - s * 0.08)], stroke);
        }
        IconKind::ArrowDown => {
            let top = Pos2::new(c.x, c.y - s * 0.30);
            let bot = Pos2::new(c.x, c.y + s * 0.30);
            painter.line_segment([top, bot], stroke);
            painter.line_segment([bot, Pos2::new(c.x - s * 0.20, c.y + s * 0.08)], stroke);
            painter.line_segment([bot, Pos2::new(c.x + s * 0.20, c.y + s * 0.08)], stroke);
        }
        IconKind::Info => {
            painter.circle_stroke(c, s * 0.38, stroke);
            painter.circle_filled(Pos2::new(c.x, c.y - s * 0.16), (s * 0.06).max(1.1), color);
            painter.line_segment(
                [
                    Pos2::new(c.x, c.y - s * 0.02),
                    Pos2::new(c.x, c.y + s * 0.20),
                ],
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
            painter.line_segment(
                [Pos2::new(rx - ah, ty - ah * 0.2), Pos2::new(rx, ty)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(rx - ah * 0.2, ty + ah), Pos2::new(rx, ty)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(rx - ah * 0.2, by - ah), Pos2::new(rx, by)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(rx - ah, by + ah * 0.2), Pos2::new(rx, by)],
                stroke,
            );
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
