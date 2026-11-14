//! A fill channel takes an image resource from its slot's source picker

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use texturecraft_engine::core::{Channel, LayerId, LayerKind, Source};
use texturecraft_ui_egui::TextureCraftApp;

#[test]
fn a_fill_channel_takes_an_image_from_its_source_menu() {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([90, 90, 90, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let name = session.import_resource_bytes("rough.png", png).unwrap();
    let out = texturecraft_engine::execute(&mut session, "layer.add.fill", &json!({"roughness": 0.6})).unwrap();
    let layer = LayerId(out["layer"].as_u64().unwrap());
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    // The labels below are English; a first run would speak the system's language.
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);

    h.get_by_role_and_label(egui::Role::Tab, "Material").click();
    h.run_steps(3);

    // The Roughness slot's row opens its source picker, whose search field has the focus and finds the image under Textures
    h.get_by_role_and_label(egui::Role::ComboBox, "Roughness").click();
    h.run_steps(2);
    h.event(egui::Event::Text("rough".into()));
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::ListBoxOption, &name).click();
    h.run_steps(3);

    let LayerKind::Fill(fill) = &h.state().session.doc.project.layers[&layer].kind else { panic!("a fill layer") };
    assert_eq!(fill.source(Channel::Roughness), Source::image(name));
}

/// An image on a fill's Normal channel offers OpenGL | DirectX in its Attributes; DirectX flips its green
#[test]
fn a_normal_image_can_be_marked_directx() {
    let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([128, 128, 255, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let name = session.import_resource_bytes("normal_dx.png", png).unwrap();
    let out = texturecraft_engine::execute(&mut session, "layer.add.fill", &json!({})).unwrap();
    let layer = LayerId(out["layer"].as_u64().unwrap());
    texturecraft_engine::execute(&mut session, "layer.fill.set", &json!({"layer": layer.0, "channel": "normal", "image": name})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));
    h.state_mut().ui.prefs.language = "en".into();
    h.run_steps(4);

    h.get_by_role_and_label(egui::Role::Tab, "Material").click();
    h.run_steps(3);

    // The normal image's convention sits in the slot's folded Attributes
    h.get_by_role_and_label(egui::Role::Button, "Attributes").scroll_to_me();

    // The scroll animates for about twenty frames
    h.run_steps(30);
    h.get_by_role_and_label(egui::Role::Button, "Attributes").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::Role::RadioButton, "DirectX").click();
    h.run_steps(3);

    let LayerKind::Fill(fill) = &h.state().session.doc.project.layers[&layer].kind else { panic!("a fill layer") };
    assert_eq!(fill.source(Channel::Normal), Source::Image { name, normal_format: texturecraft_engine::core::NormalFormat::DirectX });
}
