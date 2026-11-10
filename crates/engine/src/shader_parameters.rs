//! Export shaders parameters: the project's shader, its settings and the texture sets using it, as JSON

use serde_json::{Value, json};

use crate::Session;

/// The shader's model, its parameters and the names of `sets` (they all use the project's one shader)
pub fn shader_parameters(s: &Session, sets: &[usize]) -> Value {
    let display = &s.doc.project.display;
    let model = if display.spec_gloss { "specularGlossiness" } else { "metallicRoughness" };
    let names: Vec<&str> = sets.iter().filter_map(|ts| s.doc.project.texture_sets.get(*ts)).map(|set| set.name.as_str()).collect();

    json!({
        "shaders": [{
            "model": model,
            "parameters": {
                "parallax": display.parallax,
                "displacement": display.displacement,
                "scatterColor": display.scatter_color,
                "scatterRadius": display.scatter_radius,
                "ior": display.ior,
            },
            "textureSets": names,
        }]
    })
}

/// The JSON file's name for project `name`
pub fn shader_parameters_file(name: &str) -> String {
    format!("{}_shaders.json", texturecraft_formats::export::sanitize_file_stem(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_names_the_shader_its_settings_and_its_texture_sets() {
        let mut s = crate::sample_session(128, 32).unwrap();
        s.doc.project.display.ior = 1.33;
        let value = shader_parameters(&s, &[1]);
        let shader = &value["shaders"][0];
        assert_eq!(shader["model"], "metallicRoughness");
        assert!((shader["parameters"]["ior"].as_f64().unwrap() - 1.33).abs() < 1e-6);
        assert_eq!(shader["textureSets"], json!(["Base"]));
        assert_eq!(shader_parameters_file("Material Ball"), "Material_Ball_shaders.json");
    }
}
