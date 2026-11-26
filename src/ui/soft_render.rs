use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{Color32, ClippedPrimitive, TextureFilter, TextureId, TextureOptions, TextureWrapMode, TexturesDelta};
use eframe::epaint::{ImageData, Mesh, Primitive, Vertex, WHITE_UV};

struct SoftTexture {
    width: usize,
    height: usize,
    options: TextureOptions,
    /// Premultiplied sRGBA `[r, g, b, a]` texels matching [`Color32`].
    pixels: Vec<Color32>,
}

impl SoftTexture {
    fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 || self.pixels.is_empty() {
            return [255, 255, 255, 255];
        }

        let u = apply_wrap(u, self.options.wrap_mode);
        let v = apply_wrap(v, self.options.wrap_mode);

        // `epaint::WHITE_UV` sits inside the white pixel at texel (0, 0).
        if self.options.magnification == TextureFilter::Nearest && self.options.minification == TextureFilter::Nearest {
            let ix = ((u * self.width as f32).floor() as isize).clamp(0, self.width as isize - 1) as usize;
            let iy = ((v * self.height as f32).floor() as isize).clamp(0, self.height as isize - 1) as usize;
            return self.pixels[iy * self.width + ix].to_array();
        }

        let fx = u * self.width as f32 - 0.5;
        let fy = v * self.height as f32 - 0.5;
        let x0 = fx.floor() as isize;
        let y0 = fy.floor() as isize;
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let ix0 = wrap_index(x0, self.width, self.options.wrap_mode);
        let ix1 = wrap_index(x0 + 1, self.width, self.options.wrap_mode);
        let iy0 = wrap_index(y0, self.height, self.options.wrap_mode);
        let iy1 = wrap_index(y0 + 1, self.height, self.options.wrap_mode);

        let p00 = self.pixels[iy0 * self.width + ix0].to_array();
        let p10 = self.pixels[iy0 * self.width + ix1].to_array();
        let p01 = self.pixels[iy1 * self.width + ix0].to_array();
        let p11 = self.pixels[iy1 * self.width + ix1].to_array();

        if p00 == p10 && p00 == p01 && p00 == p11 {
            return p00;
        }

        let w00 = (1.0 - tx) * (1.0 - ty);
        let w10 = tx * (1.0 - ty);
        let w01 = (1.0 - tx) * ty;
        let w11 = tx * ty;

        [
            (p00[0] as f32 * w00 + p10[0] as f32 * w10 + p01[0] as f32 * w01 + p11[0] as f32 * w11 + 0.5) as u8,
            (p00[1] as f32 * w00 + p10[1] as f32 * w10 + p01[1] as f32 * w01 + p11[1] as f32 * w11 + 0.5) as u8,
            (p00[2] as f32 * w00 + p10[2] as f32 * w10 + p01[2] as f32 * w01 + p11[2] as f32 * w11 + 0.5) as u8,
            (p00[3] as f32 * w00 + p10[3] as f32 * w10 + p01[3] as f32 * w01 + p11[3] as f32 * w11 + 0.5) as u8,
        ]
    }
}

#[inline]
fn apply_wrap(coord: f32, mode: TextureWrapMode) -> f32 {
    match mode {
        TextureWrapMode::ClampToEdge => coord.clamp(0.0, 1.0),
        TextureWrapMode::Repeat => coord.rem_euclid(1.0),
        TextureWrapMode::MirroredRepeat => {
            let t = coord.rem_euclid(2.0);
            if t > 1.0 { 2.0 - t } else { t }
        }
    }
}

#[inline]
fn wrap_index(idx: isize, len: usize, mode: TextureWrapMode) -> usize {
    let n = len as isize;
    match mode {
        TextureWrapMode::ClampToEdge => idx.clamp(0, n - 1) as usize,
        TextureWrapMode::Repeat => idx.rem_euclid(n) as usize,
        TextureWrapMode::MirroredRepeat => {
            let m = idx.rem_euclid(2 * n);
            if m >= n { (2 * n - 1 - m) as usize } else { m as usize }
        }
    }
}

/// CPU software rasterizer for `egui` [`ClippedPrimitive`] meshes.
///
/// Renders into caller-supplied premultiplied pixel buffers (`[B, G, R, A]` for
/// Wayland `wl_shm::Format::Argb8888` and Win32 `UpdateLayeredWindow`, or
/// `[R, G, B, A]`), avoiding any OpenGL/EGL/WGL GPU driver initialization.
#[derive(Default)]
pub struct SoftCanvas {
    textures: HashMap<TextureId, SoftTexture>,
}

fn take_color_pixels(img: &mut Arc<eframe::egui::ColorImage>) -> Vec<Color32> {
    Arc::get_mut(img)
        .map(|image| std::mem::take(&mut image.pixels))
        .unwrap_or_else(|| img.pixels.clone())
}

impl SoftCanvas {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn clear_textures(&mut self) {
        self.textures.clear();
    }

    /// Apply new or updated texture regions prior to rasterization.
    pub fn update_textures_before_paint(&mut self, delta: &mut TexturesDelta) {
        for (id, deltas) in &mut delta.set {
            for d in deltas {
                let ImageData::Color(img) = &mut d.image;
                let [w, h] = img.size;
                if let Some(pos) = d.pos {
                    if let Some(tex) = self.textures.get_mut(id) {
                        tex.options = d.options;
                        let px = pos[0];
                        let py = pos[1];
                        for row in 0..h {
                            let dst_y = py + row;
                            if dst_y >= tex.height {
                                break;
                            }
                            let copy_w = w.min(tex.width.saturating_sub(px));
                            if copy_w == 0 {
                                continue;
                            }
                            let src_start = row * w;
                            let dst_start = dst_y * tex.width + px;
                            tex.pixels[dst_start..dst_start + copy_w]
                                .copy_from_slice(&img.pixels[src_start..src_start + copy_w]);
                        }
                    }
                } else if let Some(tex) = self.textures.get_mut(id) {
                    tex.width = w;
                    tex.height = h;
                    tex.options = d.options;
                    tex.pixels.clear();
                    tex.pixels = take_color_pixels(img);
                } else {
                    self.textures.insert(
                        *id,
                        SoftTexture {
                            width: w,
                            height: h,
                            options: d.options,
                            pixels: take_color_pixels(img),
                        },
                    );
                }
            }
        }
    }

    /// Free deleted textures after rasterization and drain `delta` so
    /// [`TexturesDelta::drop`] does not trigger debug assertions.
    pub fn free_textures_after_paint(&mut self, mut delta: TexturesDelta) {
        for id in delta.free.drain() {
            self.textures.remove(&id);
        }
        delta.clear();
    }

    /// Rasterize all [`ClippedPrimitive`]s into `out` (`width_px * height_px * 4` bytes).
    ///
    /// Clears `out` to transparent black (`0`) before compositing with
    /// premultiplied alpha (`ONE + ONE_MINUS_SRC_ALPHA`) in gamma space to match
    /// `egui_glow`.
    pub fn rasterize(
        &self,
        width_px: u32,
        height_px: u32,
        pixels_per_point: f32,
        primitives: &[ClippedPrimitive],
        out: &mut [u8],
        bgr_order: bool,
    ) {
        let w = width_px as usize;
        let h = height_px as usize;
        let expected = w.saturating_mul(h).saturating_mul(4);
        if w == 0 || h == 0 || out.len() < expected {
            return;
        }
        let target = &mut out[..expected];
        target.fill(0);

        let ppp = pixels_per_point.max(0.1);
        for clipped in primitives {
            let Primitive::Mesh(mesh) = &clipped.primitive else {
                continue;
            };
            if mesh.indices.len() < 3 || mesh.vertices.is_empty() {
                continue;
            }
            let clip_x0 = (clipped.clip_rect.min.x * ppp).round().clamp(0.0, width_px as f32) as i32;
            let clip_y0 = (clipped.clip_rect.min.y * ppp).round().clamp(0.0, height_px as f32) as i32;
            let clip_x1 = (clipped.clip_rect.max.x * ppp).round().clamp(0.0, width_px as f32) as i32;
            let clip_y1 = (clipped.clip_rect.max.y * ppp).round().clamp(0.0, height_px as f32) as i32;
            if clip_x1 <= clip_x0 || clip_y1 <= clip_y0 {
                continue;
            }

            let tex = self.textures.get(&mesh.texture_id);
            self.rasterize_mesh(
                mesh,
                tex,
                ppp,
                w,
                [clip_x0, clip_y0, clip_x1, clip_y1],
                target,
                bgr_order,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rasterize_mesh(
        &self,
        mesh: &Mesh,
        tex: Option<&SoftTexture>,
        ppp: f32,
        stride_px: usize,
        clip: [i32; 4],
        out: &mut [u8],
        bgr_order: bool,
    ) {
        for tri in mesh.indices.as_chunks::<3>().0 {
            let (Some(&v0), Some(&v1), Some(&v2)) = (
                mesh.vertices.get(tri[0] as usize),
                mesh.vertices.get(tri[1] as usize),
                mesh.vertices.get(tri[2] as usize),
            ) else {
                continue;
            };
            rasterize_triangle(v0, v1, v2, tex, ppp, stride_px, clip, out, bgr_order);
        }
    }
}

#[derive(Clone, Copy)]
struct ScreenPt {
    x: f32,
    y: f32,
}

#[inline(always)]
fn edge_fn(a: ScreenPt, b: ScreenPt, c: ScreenPt) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[inline(always)]
fn is_top_left(a: ScreenPt, b: ScreenPt) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dy < 0.0 || (dy == 0.0 && dx > 0.0)
}

#[inline(always)]
fn mul_div_255(a: u8, b: u8) -> u8 {
    let prod = u32::from(a) * u32::from(b) + 128;
    ((prod + (prod >> 8)) >> 8) as u8
}

#[inline(always)]
fn blend_premul_over(dst: &mut [u8], src_rgba: [u8; 4], bgr_order: bool) {
    let [sr, sg, sb, sa] = src_rgba;
    if (u32::from(sr) | u32::from(sg) | u32::from(sb) | u32::from(sa)) == 0 {
        return;
    }

    if sa == 255 {
        if bgr_order {
            dst[0] = sb;
            dst[1] = sg;
            dst[2] = sr;
            dst[3] = 255;
        } else {
            dst[0] = sr;
            dst[1] = sg;
            dst[2] = sb;
            dst[3] = 255;
        }
        return;
    }

    let inv_a = 255 - sa;
    let (dr, dg, db, da) = if bgr_order {
        (dst[2], dst[1], dst[0], dst[3])
    } else {
        (dst[0], dst[1], dst[2], dst[3])
    };

    let out_r = sr.saturating_add(mul_div_255(dr, inv_a));
    let out_g = sg.saturating_add(mul_div_255(dg, inv_a));
    let out_b = sb.saturating_add(mul_div_255(db, inv_a));
    let out_a = sa.saturating_add(mul_div_255(da, inv_a));

    if bgr_order {
        dst[0] = out_b;
        dst[1] = out_g;
        dst[2] = out_r;
        dst[3] = out_a;
    } else {
        dst[0] = out_r;
        dst[1] = out_g;
        dst[2] = out_b;
        dst[3] = out_a;
    }
}

#[allow(clippy::too_many_arguments)]
fn rasterize_triangle(
    v0: Vertex,
    mut v1: Vertex,
    mut v2: Vertex,
    tex: Option<&SoftTexture>,
    ppp: f32,
    stride_px: usize,
    clip: [i32; 4],
    out: &mut [u8],
    bgr_order: bool,
) {
    if v0.color == Color32::TRANSPARENT
        && v1.color == Color32::TRANSPARENT
        && v2.color == Color32::TRANSPARENT
    {
        return;
    }

    let p0 = ScreenPt { x: v0.pos.x * ppp, y: v0.pos.y * ppp };
    let mut p1 = ScreenPt { x: v1.pos.x * ppp, y: v1.pos.y * ppp };
    let mut p2 = ScreenPt { x: v2.pos.x * ppp, y: v2.pos.y * ppp };

    let mut area = edge_fn(p0, p1, p2);
    if area.abs() < 1e-5 {
        return;
    }
    if area < 0.0 {
        std::mem::swap(&mut v1, &mut v2);
        std::mem::swap(&mut p1, &mut p2);
        area = -area;
    }

    let [clip_x0, clip_y0, clip_x1, clip_y1] = clip;
    let min_x = (p0.x.min(p1.x).min(p2.x).floor() as i32).max(clip_x0);
    let min_y = (p0.y.min(p1.y).min(p2.y).floor() as i32).max(clip_y0);
    let max_x = (p0.x.max(p1.x).max(p2.x).ceil() as i32).min(clip_x1);
    let max_y = (p0.y.max(p1.y).max(p2.y).ceil() as i32).min(clip_y1);
    if max_x <= min_x || max_y <= min_y {
        return;
    }

    let inv_area = 1.0 / area;
    let top_left_0 = is_top_left(p1, p2);
    let top_left_1 = is_top_left(p2, p0);
    let top_left_2 = is_top_left(p0, p1);

    let const_color = (v0.color == v1.color && v1.color == v2.color).then_some(v0.color.to_array());
    let const_uv = v0.uv == v1.uv && v1.uv == v2.uv;
    let white_uv = const_uv && v0.uv == WHITE_UV;
    let const_texel = if white_uv {
        Some([255, 255, 255, 255])
    } else if const_uv {
        Some(tex.map_or([255, 255, 255, 255], |t| t.sample(v0.uv.x, v0.uv.y)))
    } else {
        None
    };

    // Fast path: axis-unconstrained solid-color & solid-texel triangle (e.g. rectangle halves).
    let const_src = match (const_color, const_texel) {
        (Some(c), Some(t)) => Some([
            mul_div_255(c[0], t[0]),
            mul_div_255(c[1], t[1]),
            mul_div_255(c[2], t[2]),
            mul_div_255(c[3], t[3]),
        ]),
        _ => None,
    };

    let c0 = v0.color.to_array();
    let c1 = v1.color.to_array();
    let c2 = v2.color.to_array();

    for py in min_y..max_y {
        let fy = py as f32 + 0.5;
        let row_offset = (py as usize) * stride_px;
        for px in min_x..max_x {
            let p = ScreenPt { x: px as f32 + 0.5, y: fy };
            let w0 = edge_fn(p1, p2, p);
            if if top_left_0 { w0 < 0.0 } else { w0 <= 0.0 } {
                continue;
            }
            let w1 = edge_fn(p2, p0, p);
            if if top_left_1 { w1 < 0.0 } else { w1 <= 0.0 } {
                continue;
            }
            let w2 = edge_fn(p0, p1, p);
            if if top_left_2 { w2 < 0.0 } else { w2 <= 0.0 } {
                continue;
            }

            let dst_idx = (row_offset + px as usize) * 4;
            if let Some(src) = const_src {
                blend_premul_over(&mut out[dst_idx..dst_idx + 4], src, bgr_order);
                continue;
            }

            let b0 = w0 * inv_area;
            let b1 = w1 * inv_area;
            let b2 = 1.0 - b0 - b1;

            let vc = if let Some(c) = const_color {
                c
            } else {
                [
                    (c0[0] as f32 * b0 + c1[0] as f32 * b1 + c2[0] as f32 * b2 + 0.5).clamp(0.0, 255.0) as u8,
                    (c0[1] as f32 * b0 + c1[1] as f32 * b1 + c2[1] as f32 * b2 + 0.5).clamp(0.0, 255.0) as u8,
                    (c0[2] as f32 * b0 + c1[2] as f32 * b1 + c2[2] as f32 * b2 + 0.5).clamp(0.0, 255.0) as u8,
                    (c0[3] as f32 * b0 + c1[3] as f32 * b1 + c2[3] as f32 * b2 + 0.5).clamp(0.0, 255.0) as u8,
                ]
            };

            let tc = if let Some(t) = const_texel {
                t
            } else {
                let u = v0.uv.x * b0 + v1.uv.x * b1 + v2.uv.x * b2;
                let v = v0.uv.y * b0 + v1.uv.y * b1 + v2.uv.y * b2;
                tex.map_or([255, 255, 255, 255], |t| t.sample(u, v))
            };

            let src = [
                mul_div_255(vc[0], tc[0]),
                mul_div_255(vc[1], tc[1]),
                mul_div_255(vc[2], tc[2]),
                mul_div_255(vc[3], tc[3]),
            ];
            blend_premul_over(&mut out[dst_idx..dst_idx + 4], src, bgr_order);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{ColorImage, Pos2, Rect};
    use eframe::epaint::ImageDelta;
    use std::sync::Arc;

    fn solid_quad_primitive(rect: Rect, color: Color32) -> ClippedPrimitive {
        let mut mesh = Mesh::default();
        let verts = [
            Vertex { pos: rect.left_top(), uv: WHITE_UV, color },
            Vertex { pos: rect.right_top(), uv: WHITE_UV, color },
            Vertex { pos: rect.right_bottom(), uv: WHITE_UV, color },
            Vertex { pos: rect.left_bottom(), uv: WHITE_UV, color },
        ];
        mesh.vertices.extend_from_slice(&verts);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }
    }

    #[test]
    fn renders_opaque_quad_in_bgra_and_rgba() {
        let canvas = SoftCanvas::new();
        let color = Color32::from_rgba_premultiplied(20, 40, 200, 255);
        let prim = solid_quad_primitive(
            Rect::from_min_max(Pos2::new(2.0, 2.0), Pos2::new(6.0, 5.0)),
            color,
        );

        let mut bgra = vec![0u8; 8 * 8 * 4];
        canvas.rasterize(8, 8, 1.0, std::slice::from_ref(&prim), &mut bgra, true);
        let idx = (3 * 8 + 4) * 4;
        assert_eq!(&bgra[idx..idx + 4], &[200, 40, 20, 255]);
        assert_eq!(&bgra[0..4], &[0, 0, 0, 0]);

        let mut rgba = vec![0u8; 8 * 8 * 4];
        canvas.rasterize(8, 8, 1.0, &[prim], &mut rgba, false);
        assert_eq!(&rgba[idx..idx + 4], &[20, 40, 200, 255]);
    }

    #[test]
    fn translucent_shared_diagonal_never_double_blends() {
        let canvas = SoftCanvas::new();
        let color = Color32::from_rgba_premultiplied(100, 0, 0, 100);
        let prim = solid_quad_primitive(
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(8.0, 8.0)),
            color,
        );
        let mut rgba = vec![0u8; 8 * 8 * 4];
        canvas.rasterize(8, 8, 1.0, &[prim], &mut rgba, false);

        for y in 0..8usize {
            for x in 0..8usize {
                let idx = (y * 8 + x) * 4;
                assert_eq!(
                    &rgba[idx..idx + 4],
                    &[100, 0, 0, 100],
                    "pixel ({x},{y}) showed double blend or gap"
                );
            }
        }
    }

    #[test]
    fn clip_rect_and_hidpi_scale_clip_accurately() {
        let canvas = SoftCanvas::new();
        let prim = ClippedPrimitive {
            clip_rect: Rect::from_min_max(Pos2::new(1.0, 1.0), Pos2::new(3.0, 3.0)),
            primitive: solid_quad_primitive(
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(5.0, 5.0)),
                Color32::from_rgb(10, 20, 30),
            )
            .primitive,
        };
        let mut rgba = vec![0u8; 10 * 10 * 4];
        canvas.rasterize(10, 10, 2.0, &[prim], &mut rgba, false);

        // Scaled by 2.0 => physical clip rect is [2..6, 2..6].
        assert_eq!(&rgba[(10 + 1) * 4..(10 + 1) * 4 + 4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[(2 * 10 + 2) * 4..(2 * 10 + 2) * 4 + 4], &[10, 20, 30, 255]);
        assert_eq!(&rgba[(5 * 10 + 5) * 4..(5 * 10 + 5) * 4 + 4], &[10, 20, 30, 255]);
        assert_eq!(&rgba[(6 * 10 + 6) * 4..(6 * 10 + 6) * 4 + 4], &[0, 0, 0, 0]);
    }

    #[test]
    fn textured_mesh_partial_update_and_free_work() {
        let mut canvas = SoftCanvas::new();
        let tex_id = TextureId::Managed(1);
        let full = ColorImage::filled([2, 2], Color32::from_rgb(0, 0, 255));
        let mut delta = TexturesDelta::default();
        delta.set.insert(
            tex_id,
            vec![ImageDelta::full(full, TextureOptions::NEAREST)].into(),
        );
        canvas.update_textures_before_paint(&mut delta);
        canvas.free_textures_after_paint(delta);

        // Partial update changes pixel (1, 1) to green.
        let mut patch_delta = TexturesDelta::default();
        patch_delta.set.insert(
            tex_id,
            vec![ImageDelta::partial(
                [1, 1],
                ColorImage::filled([1, 1], Color32::from_rgb(0, 255, 0)),
                TextureOptions::NEAREST,
            )]
            .into(),
        );
        canvas.update_textures_before_paint(&mut patch_delta);
        canvas.free_textures_after_paint(patch_delta);

        let mut mesh = Mesh::with_texture(tex_id);
        let white = Color32::WHITE;
        mesh.vertices.extend_from_slice(&[
            Vertex { pos: Pos2::new(0.0, 0.0), uv: Pos2::new(0.0, 0.0), color: white },
            Vertex { pos: Pos2::new(4.0, 0.0), uv: Pos2::new(1.0, 0.0), color: white },
            Vertex { pos: Pos2::new(4.0, 4.0), uv: Pos2::new(1.0, 1.0), color: white },
            Vertex { pos: Pos2::new(0.0, 4.0), uv: Pos2::new(0.0, 1.0), color: white },
        ]);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        let prim = ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        };
        let mut rgba = vec![0u8; 4 * 4 * 4];
        canvas.rasterize(4, 4, 1.0, &[prim], &mut rgba, false);

        // Top-left (0, 0) sampled texel (0, 0) = blue; bottom-right (3, 3) sampled texel (1, 1) = green.
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255]);
        assert_eq!(&rgba[(3 * 4 + 3) * 4..(3 * 4 + 3) * 4 + 4], &[0, 255, 0, 255]);

        let mut free_delta = TexturesDelta::default();
        free_delta.free.insert(tex_id);
        canvas.free_textures_after_paint(free_delta);
        assert!(!canvas.textures.contains_key(&tex_id));

        // Silence unused import warning when only ColorImage is referenced directly.
        let _ = Arc::<ColorImage>::new;
    }
}
