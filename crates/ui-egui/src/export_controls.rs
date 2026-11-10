//! The Export textures window's parameter controls and value texts, shared by Global settings and a texture set's overrides

use egui::Ui;
use texturecraft_engine::formats::FileFormat;

use crate::i18n::{tr, trf};

/// File types offered besides the template's own
const FILE_TYPES: [&str; 8] = ["png", "jpeg", "tga", "tiff", "bmp", "webp", "exr", "hdr"];
/// Output sizes offered besides each texture set's own
const SIZES: [u32; 6] = [256, 512, 1024, 2048, 4096, 8192];
/// Padding modes in menu order
const PADDINGS: [&str; 4] = ["infinite", "dilation", "dilationDefault", "none"];
const DILATION_MAX: u32 = 256;

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

pub fn file_type_text(format: &str) -> String {
    if format.is_empty() { tr("From template").to_string() } else { format.to_string() }
}

pub fn bit_depth_text(bit_depth: u8) -> String {
    match bit_depth {
        0 => tr("Default").to_string(),
        32 => tr("32 bits (float)").to_string(),
        n => trf("{n} bits", &[("n", &n)]),
    }
}

pub fn file_type_combos(ui: &mut Ui, id: &str, format: &mut String, bit_depth: &mut u8) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt((id, "format")).selected_text(file_type_text(format)).show_ui(ui, |ui| {
            ui.selectable_value(format, String::new(), tr("From template"));
            for extension in FILE_TYPES {
                ui.selectable_value(format, extension.to_string(), extension);
            }
        });
        egui::ComboBox::from_id_salt((id, "bits")).selected_text(bit_depth_text(*bit_depth)).show_ui(ui, |ui| {
            for depth in [0u8, 8, 16, 32] {
                ui.selectable_value(bit_depth, depth, bit_depth_text(depth));
            }
        });
    });
}

/// One output map's own file type: a format and one of the bit depths it holds
pub fn map_file_type_combos(ui: &mut Ui, id: &str, extension: &mut String, bit_depth: &mut u8) {
    let format = FileFormat::from_extension(extension).unwrap_or(FileFormat::Png);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt((id, "format")).width(64.0).selected_text(format.extension()).show_ui(ui, |ui| {
            for choice in FileFormat::ALL {
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
