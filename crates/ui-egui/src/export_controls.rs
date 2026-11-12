//! The Export textures window's parameter controls and value texts, shared by Global settings and a texture set's overrides

use egui::{RichText, Ui, vec2};
use texturecraft_engine::core::ExportFileType;
use texturecraft_engine::formats::{ExportPreset, FileFormat};

use crate::i18n::{tr, trf};

/// Built-in templates that aren't for one engine or renderer: listed first, apart from the rest, as the reference does
const GENERAL_TEMPLATES: [&str; 5] = ["Document channels + Normal + AO", "glTF PBR Metal Roughness", "OpenPBR (separate maps)", "Separate maps", "USDZ (AR)"];
/// File types offered besides the template's own, in menu order
const FILE_TYPES: [FileFormat; 10] = [
    FileFormat::Bmp,
    FileFormat::Jpeg,
    FileFormat::Jng,
    FileFormat::Pgm,
    FileFormat::Png,
    FileFormat::Tga,
    FileFormat::Tiff,
    FileFormat::Hdr,
    FileFormat::Exr,
    FileFormat::Webp,
];
/// Output sizes offered besides each texture set's own
pub(crate) const SIZES: [u32; 7] = [128, 256, 512, 1024, 2048, 4096, 8192];
/// Padding modes in menu order
const PADDINGS: [&str; 5] = ["none", "infinite", "dilation", "dilationDefault", "diffusion"];
const DILATION_MAX: u32 = 256;
/// The bit depth a file type starts at when picked
const DEFAULT_BIT_DEPTH: u8 = 8;
const BIT_DEPTH_WIDTH: f32 = 110.0;
/// The file type combo's width; its bit depth combo takes the rest
const FILE_TYPE_WIDTH: f32 = 165.0;
const DILATION_FIELD_WIDTH: f32 = 64.0;
/// A texture set map's own file type combo: wide enough for the longest extension
const MAP_FORMAT_WIDTH: f32 = 64.0;

pub fn template_combo(ui: &mut Ui, id: &str, preset: &mut String, builtins: &[String], user: &[String], width: f32) {
    egui::ComboBox::from_id_salt(id).width(width).selected_text(preset.clone()).show_ui(ui, |ui| {
        for (index, name) in builtins.iter().enumerate() {
            if starts_template_group(builtins, index) {
                ui.separator();
            }
            ui.selectable_value(preset, name.clone(), name);
        }
        if !user.is_empty() {
            ui.separator();
        }
        for name in user {
            ui.selectable_value(preset, name.clone(), name);
        }
    });
}

/// Built-in templates in the order both lists show them: the general ones, then the rest, each group alphabetical
pub fn sort_templates(presets: &mut [ExportPreset]) {
    presets.sort_by_key(|preset| (!GENERAL_TEMPLATES.contains(&preset.name.as_str()), preset.name.to_lowercase()));
}

/// Whether the sorted template `index` of `names` is the first one after the general group
pub fn starts_template_group(names: &[impl AsRef<str>], index: usize) -> bool {
    let general = |index: usize| names.get(index).is_some_and(|name| GENERAL_TEMPLATES.contains(&name.as_ref()));
    index > 0 && general(index - 1) && !general(index)
}

/// A file type's name in Settings (the extension elsewhere)
fn file_type_name(format: FileFormat) -> &'static str {
    match format {
        FileFormat::Jpeg => "jpeg",
        FileFormat::Tga => "targa",
        FileFormat::Tiff => "tiff",
        other => other.extension(),
    }
}

/// An empty extension is the template's own file types
pub fn file_type_text(extension: &str) -> String {
    match FileFormat::from_extension(extension) {
        Some(format) => file_type_name(format).to_string(),
        None => tr("Based on output template").to_string(),
    }
}

/// "8 bits", "8 bits + dithering", "32 bits (float)"
pub fn bit_depth_text(bit_depth: u8, dithering: bool) -> String {
    match (bit_depth, dithering) {
        (0, _) => tr("Default").to_string(),
        (32, _) => tr("32 bits (float)").to_string(),
        (8, true) => tr("8 bits + dithering").to_string(),
        (n, _) => trf("{n} bits", &[("n", &n)]),
    }
}

/// The bit depths `format` holds, with 8 bits also offered dithered
fn depth_choices(format: FileFormat) -> Vec<(u8, bool)> {
    let mut choices = Vec::new();
    for &depth in format.bit_depths() {
        choices.push((depth, false));
        if depth == 8 {
            choices.push((depth, true));
        }
    }

    choices
}

fn depth_combo(ui: &mut Ui, id: &str, format: FileFormat, file_type: &mut ExportFileType, width: f32) {
    let shown = bit_depth_text(file_type.bit_depth, file_type.dithering);
    egui::ComboBox::from_id_salt((id, "bits")).width(width).selected_text(shown).show_ui(ui, |ui| {
        for (depth, dithering) in depth_choices(format) {
            let selected = file_type.bit_depth == depth && file_type.dithering == dithering;
            if ui.selectable_label(selected, bit_depth_text(depth, dithering)).clicked() {
                file_type.bit_depth = depth;
                file_type.dithering = dithering;
            }
        }
    });
}

/// `file_type` now writing `format`, at the nearest bit depth it holds
fn switch_format(file_type: &mut ExportFileType, format: FileFormat) {
    file_type.format = format.extension().to_string();
    file_type.bit_depth = format.depth(if file_type.bit_depth == 0 { DEFAULT_BIT_DEPTH } else { file_type.bit_depth });
    file_type.dithering &= file_type.bit_depth == 8;
}

/// The file type, then a bit depth it holds; the bit depth is the template's while the file type is
pub fn file_type_combos(ui: &mut Ui, id: &str, file_type: &mut ExportFileType, width: f32) {
    let chosen = FileFormat::from_extension(&file_type.format);
    // The bit depth takes the rest of `width`; each combo adds room for its arrow and padding, so one combo's comes off
    let arrow = ui.spacing().icon_width + ui.spacing().icon_spacing + ui.spacing().button_padding.x;
    let rest = width - FILE_TYPE_WIDTH - ui.spacing().item_spacing.x - arrow;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt((id, "format")).width(FILE_TYPE_WIDTH).selected_text(file_type_text(&file_type.format)).show_ui(ui, |ui| {
            if ui.selectable_label(chosen.is_none(), tr("Based on output template")).clicked() {
                *file_type = ExportFileType::default();
            }
            for format in FILE_TYPES {
                if ui.selectable_label(chosen == Some(format), file_type_name(format)).clicked() {
                    switch_format(file_type, format);
                }
            }
        });
        match chosen {
            Some(format) => depth_combo(ui, id, format, file_type, rest),
            None => {
                ui.add_enabled_ui(false, |ui| egui::ComboBox::from_id_salt((id, "bits")).width(rest).selected_text("").show_ui(ui, |_| {}));
            }
        }
    });
}

/// "png  8 bits", or the template's file types
pub fn file_type_with_depth_text(file_type: &ExportFileType) -> String {
    if file_type.format.is_empty() {
        return file_type_text(&file_type.format);
    }

    format!("{}  {}", file_type_text(&file_type.format), bit_depth_text(file_type.bit_depth, file_type.dithering))
}

/// One output map's own file type: a format and one of the bit depths it holds
pub fn map_file_type_combos(ui: &mut Ui, id: &str, file_type: &mut ExportFileType) {
    let format = FileFormat::from_extension(&file_type.format).unwrap_or(FileFormat::Png);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt((id, "format")).width(MAP_FORMAT_WIDTH).selected_text(format.extension()).show_ui(ui, |ui| {
            for choice in FILE_TYPES {
                if ui.selectable_label(choice == format, choice.extension()).clicked() {
                    switch_format(file_type, choice);
                }
            }
        });
        depth_combo(ui, id, format, file_type, BIT_DEPTH_WIDTH);
    });
}

/// `set_size` is the texture set's own size, shown for "based on each texture set's size" when known
pub fn size_text(size: u32, set_size: Option<(u32, u32)>) -> String {
    match (size, set_size) {
        (0, Some((width, height))) => trf("{width}x{height} (texture set size)", &[("width", &width), ("height", &height)]),
        (0, None) => tr("Based on each texture set's size").to_string(),
        (size, _) => format!("{size} x {size}"),
    }
}

pub fn size_combo(ui: &mut Ui, id: &str, size: &mut u32, set_size: Option<(u32, u32)>, width: f32) {
    egui::ComboBox::from_id_salt(id).width(width).selected_text(size_text(*size, set_size)).show_ui(ui, |ui| {
        ui.selectable_value(size, 0, size_text(0, set_size));
        ui.separator();
        ui.label(RichText::new(tr("Maximum size for all texture sets")).italics().weak());
        for pixels in SIZES {
            ui.selectable_value(size, pixels, size_text(pixels, set_size));
        }
    });
}

pub fn padding_label(padding: &str) -> &'static str {
    match padding {
        "none" => tr("No padding (passthrough)"),
        "dilation" => tr("Dilation + transparent"),
        "dilationDefault" => tr("Dilation + default background color"),
        "diffusion" => tr("Dilation + diffusion"),
        _ => tr("Dilation infinite"),
    }
}

/// Whether `padding` dilates a set distance (and so shows it)
fn has_distance(padding: &str) -> bool {
    matches!(padding, "dilation" | "dilationDefault" | "diffusion")
}

pub fn padding_text(padding: &str, dilation: u32) -> String {
    if has_distance(padding) { format!("{} · {dilation} px", padding_label(padding)) } else { padding_label(padding).to_string() }
}

pub fn padding_controls(ui: &mut Ui, id: &str, padding: &mut String, dilation: &mut u32, width: f32) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id).width(width - DILATION_FIELD_WIDTH - ui.spacing().item_spacing.x).selected_text(padding_label(padding)).show_ui(
            ui,
            |ui| {
                for mode in PADDINGS {
                    ui.selectable_value(padding, mode.to_string(), padding_label(mode));
                }
            },
        );
        ui.add_enabled_ui(has_distance(padding), |ui| {
            ui.add_sized(vec2(DILATION_FIELD_WIDTH, ui.spacing().interact_size.y), egui::DragValue::new(dilation).range(0..=DILATION_MAX).suffix(" px"))
        });
    });
}
