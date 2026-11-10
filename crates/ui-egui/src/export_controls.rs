//! The Export textures window's parameter controls and value texts, shared by Global settings and a texture set's overrides

use egui::{RichText, Ui};
use texturecraft_engine::formats::FileFormat;

use crate::i18n::{tr, trf};

/// File types offered besides the template's own, in menu order
const FILE_TYPES: [FileFormat; 8] =
    [FileFormat::Bmp, FileFormat::Jpeg, FileFormat::Png, FileFormat::Tga, FileFormat::Tiff, FileFormat::Hdr, FileFormat::Exr, FileFormat::Webp];
/// Output sizes offered besides each texture set's own
const SIZES: [u32; 7] = [128, 256, 512, 1024, 2048, 4096, 8192];
/// Padding modes in menu order
const PADDINGS: [&str; 4] = ["infinite", "dilation", "dilationDefault", "none"];
const DILATION_MAX: u32 = 256;
/// The bit depth a file type starts at when picked
const DEFAULT_BIT_DEPTH: u8 = 8;
const BIT_DEPTH_WIDTH: f32 = 110.0;

pub fn template_combo(ui: &mut Ui, id: &str, preset: &mut String, builtins: &[String], user: &[String]) {
    egui::ComboBox::from_id_salt(id).width(360.0).selected_text(preset.clone()).show_ui(ui, |ui| {
        for name in builtins {
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

pub fn bit_depth_text(bit_depth: u8) -> String {
    match bit_depth {
        0 => tr("Default").to_string(),
        32 => tr("32 bits (float)").to_string(),
        n => trf("{n} bits", &[("n", &n)]),
    }
}

/// The file type, then a bit depth it holds; the bit depth is the template's while the file type is
pub fn file_type_combos(ui: &mut Ui, id: &str, extension: &mut String, bit_depth: &mut u8) {
    let chosen = FileFormat::from_extension(extension);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt((id, "format")).selected_text(file_type_text(extension)).show_ui(ui, |ui| {
            if ui.selectable_label(chosen.is_none(), tr("Based on output template")).clicked() {
                extension.clear();
                *bit_depth = 0;
            }
            for format in FILE_TYPES {
                if ui.selectable_label(chosen == Some(format), file_type_name(format)).clicked() {
                    *extension = format.extension().to_string();
                    *bit_depth = format.depth(if *bit_depth == 0 { DEFAULT_BIT_DEPTH } else { *bit_depth });
                }
            }
        });
        ui.add_enabled_ui(chosen.is_some(), |ui| {
            let shown = if chosen.is_some() { bit_depth_text(*bit_depth) } else { String::new() };
            egui::ComboBox::from_id_salt((id, "bits")).width(BIT_DEPTH_WIDTH).selected_text(shown).show_ui(ui, |ui| {
                for &depth in chosen.map(FileFormat::bit_depths).unwrap_or_default() {
                    ui.selectable_value(bit_depth, depth, bit_depth_text(depth));
                }
            });
        });
    });
}

/// "png 8 bits", or the template's file types
pub fn file_type_with_depth_text(extension: &str, bit_depth: u8) -> String {
    if extension.is_empty() { file_type_text(extension) } else { format!("{}  {}", file_type_text(extension), bit_depth_text(bit_depth)) }
}

/// One output map's own file type: a format and one of the bit depths it holds
pub fn map_file_type_combos(ui: &mut Ui, id: &str, extension: &mut String, bit_depth: &mut u8) {
    let format = FileFormat::from_extension(extension).unwrap_or(FileFormat::Png);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt((id, "format")).width(64.0).selected_text(format.extension()).show_ui(ui, |ui| {
            for choice in FILE_TYPES {
                if ui.selectable_label(choice == format, choice.extension()).clicked() {
                    *extension = choice.extension().to_string();
                    *bit_depth = choice.depth(*bit_depth);
                }
            }
        });
        egui::ComboBox::from_id_salt((id, "bits")).selected_text(bit_depth_text(*bit_depth)).show_ui(ui, |ui| {
            for &depth in format.bit_depths() {
                ui.selectable_value(bit_depth, depth, bit_depth_text(depth));
            }
        });
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

pub fn size_combo(ui: &mut Ui, id: &str, size: &mut u32, set_size: Option<(u32, u32)>) {
    egui::ComboBox::from_id_salt(id).width(360.0).selected_text(size_text(*size, set_size)).show_ui(ui, |ui| {
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
        _ => tr("Dilation infinite"),
    }
}

pub fn padding_text(padding: &str, dilation: u32) -> String {
    if padding.starts_with("dilation") { format!("{} · {dilation} px", padding_label(padding)) } else { padding_label(padding).to_string() }
}

pub fn padding_controls(ui: &mut Ui, id: &str, padding: &mut String, dilation: &mut u32) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id).width(260.0).selected_text(padding_label(padding)).show_ui(ui, |ui| {
            for mode in PADDINGS {
                ui.selectable_value(padding, mode.to_string(), padding_label(mode));
            }
        });
        if padding.starts_with("dilation") {
            ui.add(egui::DragValue::new(dilation).range(0..=DILATION_MAX).suffix(" px"));
        }
    });
}
