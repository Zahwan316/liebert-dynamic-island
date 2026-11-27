use std::sync::Arc;

use egui::FontId;
use epaint::text::{FontInsert, FontPriority, InsertFontFamily};

use crate::platform::light as platform_light;
use crate::platform::light::{FontFamilyTarget, FontFileSpec, InstalledFont};

struct MmapFont(memmap2::Mmap);

impl AsRef<[u8]> for MmapFont {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub fn load_font_blob(path: &str) -> Option<Arc<dyn AsRef<[u8]> + Send + Sync>> {
    if let Ok(file) = std::fs::File::open(path) {
        // SAFETY: system font files are immutable during runtime; fallback to fs::read if mmap fails.
        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
            return Some(Arc::new(MmapFont(mmap)));
        }
    }
    std::fs::read(path)
        .ok()
        .map(|bytes| Arc::new(bytes) as Arc<dyn AsRef<[u8]> + Send + Sync>)
}

pub fn lyrics_bold_family() -> egui::FontFamily {
    egui::FontFamily::Name(Arc::from("LyricsBold"))
}

pub fn lyrics_bold_font(size: f32) -> FontId {
    FontId::new(size, lyrics_bold_family())
}

pub fn ui_bold_font(size: f32) -> FontId {
    lyrics_bold_font(size)
}

fn insert_font_file_into_family(
    defs: &mut egui::FontDefinitions,
    family: &egui::FontFamily,
    insert_at_front: bool,
    key: String,
    blob: Arc<dyn AsRef<[u8]> + Send + Sync>,
    face_index: u32,
) {
    defs.font_data.insert(
        key.clone(),
        Arc::new(egui::FontData::from_blob(blob, face_index)),
    );
    if let Some(fam) = defs.families.get_mut(family) {
        if insert_at_front {
            fam.insert(0, key);
        } else {
            fam.push(key);
        }
    }
}

pub fn find_bold_sibling_font<'a>(
    installed: &'a [InstalledFont],
    selected_name: &str,
) -> Option<&'a InstalledFont> {
    let lower = selected_name.to_ascii_lowercase();
    let space_bold = format!("{lower} bold");
    let dash_bold = format!("{lower}-bold");
    installed
        .iter()
        .find(|f| {
            let n = f.name.to_ascii_lowercase();
            n == space_bold || n == dash_bold
        })
        .or_else(|| {
            installed
                .iter()
                .filter(|f| {
                    let n = f.name.to_ascii_lowercase();
                    n.starts_with(&lower) && n.contains("bold")
                })
                .min_by_key(|f| f.name.len())
        })
}

#[allow(dead_code)]
pub fn apply_configured_fonts(ctx: &egui::Context, font_name: &str) {
    apply_configured_fonts_with_options(ctx, font_name, true);
}

pub fn apply_configured_fonts_light(ctx: &egui::Context, font_name: &str) {
    apply_configured_fonts_with_options(ctx, font_name, false);
}

fn apply_configured_fonts_with_options(
    ctx: &egui::Context,
    font_name: &str,
    include_bold_lyrics: bool,
) {
    let mut defs = egui::FontDefinitions::empty();
    defs.families
        .insert(egui::FontFamily::Proportional, Vec::new());
    defs.families
        .insert(egui::FontFamily::Monospace, Vec::new());
    defs.families.insert(lyrics_bold_family(), Vec::new());

    let trimmed = font_name.trim();
    let mut user_selected_regular_fallback: Option<String> = None;

    if !trimmed.is_empty() {
        let installed = platform_light::list_installed_fonts();
        if let Some(found) = installed
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(trimmed))
            && let Some(blob) = load_font_blob(&found.path)
        {
            let selected_is_bold = trimmed.to_ascii_lowercase().contains("bold");
            insert_font_file_into_family(
                &mut defs,
                &egui::FontFamily::Proportional,
                true,
                "user_selected_font".to_owned(),
                blob.clone(),
                0u32,
            );
            if selected_is_bold {
                if let Some(fam) = defs.families.get_mut(&lyrics_bold_family()) {
                    fam.push("user_selected_font".to_owned());
                }
            } else if let Some(bold_sib) = find_bold_sibling_font(&installed, trimmed)
                && let Some(bold_blob) = load_font_blob(&bold_sib.path)
            {
                insert_font_file_into_family(
                    &mut defs,
                    &lyrics_bold_family(),
                    false,
                    "user_selected_bold_font".to_owned(),
                    bold_blob,
                    0u32,
                );
            } else {
                user_selected_regular_fallback = Some("user_selected_font".to_owned());
            }
        }
    }

    // Always register the lightweight primary system Bold face for expanded headers, bold clock,
    // and notification titles even before heavy CJK bold lyric fonts are loaded lazily.
    let bold_specs = platform_light::bold_fallback_fonts();
    let bold_count = if include_bold_lyrics {
        bold_specs.len()
    } else {
        1
    };
    for spec in bold_specs.into_iter().take(bold_count) {
        if let Some(blob) = load_font_blob(&spec.path) {
            for &face_index in spec.face_indices {
                let name = if spec.face_indices.len() == 1 && face_index == 0 {
                    spec.name.clone()
                } else {
                    format!("{}_{}", spec.name, face_index)
                };
                let font_data = egui::FontData::from_blob(blob.clone(), face_index);
                defs.font_data.insert(name.clone(), Arc::new(font_data));
                if let Some(fam) = defs.families.get_mut(&lyrics_bold_family()) {
                    fam.push(name);
                }
            }
        }
    }
    if let Some(key) = user_selected_regular_fallback
        && let Some(fam) = defs.families.get_mut(&lyrics_bold_family())
    {
        fam.push(key);
    }

    ctx.set_fonts(defs);
    install_base_fallback_fonts(ctx);
}

pub fn install_base_fallback_fonts(ctx: &egui::Context) {
    for mut spec in platform_light::base_fallback_fonts() {
        if spec.family == FontFamilyTarget::Proportional {
            // Also register system sans as fallback for LyricsBold so LyricsBold works even before
            // bold fonts are lazily loaded for music playback.
            spec.family = FontFamilyTarget::All;
        }
        load_font_spec(ctx, &spec, FontPriority::Lowest);
    }
}

pub fn load_font_spec(ctx: &egui::Context, spec: &FontFileSpec, priority: FontPriority) -> bool {
    let Some(blob) = load_font_blob(&spec.path) else {
        return false;
    };
    let families_base = match spec.family {
        FontFamilyTarget::Proportional => vec![InsertFontFamily {
            family: egui::FontFamily::Proportional,
            priority,
        }],
        FontFamilyTarget::Monospace => vec![InsertFontFamily {
            family: egui::FontFamily::Monospace,
            priority,
        }],
        FontFamilyTarget::Both => vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority,
            },
        ],
        FontFamilyTarget::Bold => vec![InsertFontFamily {
            family: lyrics_bold_family(),
            priority,
        }],
        FontFamilyTarget::All => vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority,
            },
            InsertFontFamily {
                family: lyrics_bold_family(),
                priority,
            },
        ],
    };

    for &face_index in spec.face_indices {
        let name = if spec.face_indices.len() == 1 && face_index == 0 {
            spec.name.clone()
        } else {
            format!("{}_{}", spec.name, face_index)
        };
        let font_data = egui::FontData::from_blob(blob.clone(), face_index);
        ctx.add_font(FontInsert::new(&name, font_data, families_base.clone()));
    }
    true
}
