//! Presets for engines and renderers, each laid out as that target's own documentation asks for its textures

use texturecraft_core::Channel;

use super::{base_with_alpha, ch, gray, map, normal, orm, rgb, virt};
use crate::export::{Dest, ExportPreset, MapSpec, SourceRef, VirtualMap};

/// A grey 8-bit map of channel `channel`
fn gray8(suffix: &str, channel: Channel) -> MapSpec {
    map(suffix, 8, gray(ch(channel, 0)))
}

/// An RGB 8-bit map of colour channel `channel`
fn rgb8(suffix: &str, channel: Channel) -> MapSpec {
    map(suffix, 8, rgb(|k| ch(channel, k)))
}

/// An RGB 8-bit map of converted map `converted`
fn converted8(suffix: &str, converted: VirtualMap) -> MapSpec {
    map(suffix, 8, rgb(|k| virt(converted, k)))
}

/// A normal map in the project's own convention, for targets whose documentation doesn't name one
fn project_normal(suffix: &str) -> MapSpec {
    map(suffix, 8, rgb(|k| ch(Channel::Normal, k)))
}

/// Height in 16 bits
fn height(suffix: &str) -> MapSpec {
    map(suffix, 16, gray(ch(Channel::Height, 0)))
}

/// Unity's metallic mask: metallic, ambient occlusion, an empty detail mask and smoothness (glossiness)
fn unity_mask(suffix: &str) -> MapSpec {
    map(
        suffix,
        8,
        vec![
            (Dest::R, ch(Channel::Metallic, 0)),
            (Dest::G, virt(VirtualMap::AoMixed, 0)),
            (Dest::B, SourceRef::Constant(0.0)),
            (Dest::A, virt(VirtualMap::Glossiness, 0)),
        ],
    )
}

pub(super) fn engine_presets() -> Vec<ExportPreset> {
    let preset = |name: &str, maps: Vec<MapSpec>| ExportPreset { name: name.into(), maps };
    vec![
        preset(
            "PBR Metallic Roughness",
            vec![
                map("BaseColor", 8, base_with_alpha()),
                gray8("Metallic", Channel::Metallic),
                gray8("Roughness", Channel::Roughness),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                height("Height"),
                rgb8("Emissive", Channel::Emissive),
                map("AmbientOcclusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
            ],
        ),
        // Unity HDRP Lit, specular color material type: the mask map keeps occlusion and smoothness
        preset(
            "Unity HDRP (Specular)",
            vec![
                map("BaseMap", 8, base_with_alpha()),
                converted8("SpecularColor", VirtualMap::SpecularColor),
                unity_mask("MaskMap"),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                rgb8("Emissive", Channel::Emissive),
                height("Height"),
            ],
        ),
        // Unity URP Lit: one channel-packed map for metallic, occlusion and smoothness
        preset(
            "Unity URP (Metallic)",
            vec![
                map("BaseMap", 8, base_with_alpha()),
                unity_mask("MetallicSmoothness"),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                rgb8("Emission", Channel::Emissive),
                height("Height"),
            ],
        ),
        // Unity URP Lit, specular workflow: specular color with smoothness in its alpha
        preset(
            "Unity URP (Specular)",
            vec![
                map("BaseMap", 8, base_with_alpha()),
                map("Specular", 8, rgb(|k| virt(VirtualMap::SpecularColor, k)).into_iter().chain([(Dest::A, virt(VirtualMap::Glossiness, 0))]).collect()),
                map("Occlusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                rgb8("Emission", Channel::Emissive),
                height("Height"),
            ],
        ),
        // Unreal's subsurface profile reads its scattering mask from the material's opacity input
        preset(
            "Unreal Engine SSS (Packed)",
            vec![
                rgb8("BaseColor", Channel::BaseColor),
                map("OcclusionRoughnessMetallic", 8, orm()),
                map("Normal", 8, normal(VirtualMap::NormalDirectX)),
                gray8("SSS", Channel::ScatterWeight),
            ],
        ),
        // CryEngine Illum: diffuse (black where metal), DirectX normal with gloss in alpha, specular color
        preset(
            "CryEngine (Illum)",
            vec![
                converted8("diff", VirtualMap::Diffuse),
                map("ddna", 8, rgb(|k| virt(VirtualMap::NormalDirectX, k)).into_iter().chain([(Dest::A, virt(VirtualMap::Glossiness, 0))]).collect()),
                converted8("spec", VirtualMap::SpecularColor),
            ],
        ),
        // Lens Studio PBR: material params pack metallic, roughness and occlusion
        preset(
            "Lens Studio (PBR)",
            vec![
                rgb8("BaseColor", Channel::BaseColor),
                gray8("Opacity", Channel::Opacity),
                map(
                    "MaterialParams",
                    8,
                    vec![(Dest::R, ch(Channel::Metallic, 0)), (Dest::G, ch(Channel::Roughness, 0)), (Dest::B, virt(VirtualMap::AoMixed, 0))],
                ),
                project_normal("Normal"),
                rgb8("Emissive", Channel::Emissive),
            ],
        ),
        // Arnold Standard Surface: one map per parameter
        preset(
            "Arnold (Standard Surface)",
            vec![
                rgb8("base_color", Channel::BaseColor),
                gray8("metalness", Channel::Metallic),
                gray8("specular_roughness", Channel::Roughness),
                converted8("specular_color", VirtualMap::SpecularColor),
                gray8("transmission", Channel::Transmission),
                gray8("subsurface", Channel::ScatterWeight),
                gray8("coat", Channel::CoatWeight),
                gray8("coat_roughness", Channel::CoatRoughness),
                gray8("sheen", Channel::SheenWeight),
                rgb8("sheen_color", Channel::SheenColor),
                gray8("sheen_roughness", Channel::SheenRoughness),
                rgb8("emission_color", Channel::Emissive),
                gray8("opacity", Channel::Opacity),
                map("normal", 8, normal(VirtualMap::NormalOpenGl)),
                height("height"),
            ],
        ),
        // V-Ray, metalness: base color as diffuse, a white reflection, roughness and metalness
        preset(
            "V-Ray (Metal Roughness)",
            vec![
                rgb8("diffuse", Channel::BaseColor),
                map("reflection", 8, gray(SourceRef::Constant(1.0))),
                gray8("roughness", Channel::Roughness),
                gray8("metalness", Channel::Metallic),
                project_normal("normal"),
            ],
        ),
        // V-Ray, glossiness: diffuse, the reflection color and glossiness
        preset(
            "V-Ray (Specular Glossiness)",
            vec![
                converted8("diffuse", VirtualMap::Diffuse),
                converted8("reflection", VirtualMap::SpecularColor),
                map("glossiness", 8, gray(virt(VirtualMap::Glossiness, 0))),
                project_normal("normal"),
            ],
        ),
        // O3DE (Lumberyard's successor) picks each texture's role from its file name suffix
        preset(
            "O3DE / Lumberyard (StandardPBR)",
            vec![
                rgb8("basecolor", Channel::BaseColor),
                gray8("metallic", Channel::Metallic),
                gray8("roughness", Channel::Roughness),
                project_normal("normal"),
                map("ao", 8, gray(virt(VirtualMap::AoMixed, 0))),
                rgb8("emissive", Channel::Emissive),
                gray8("opacity", Channel::Opacity),
                height("height"),
            ],
        ),
        // USD preview surface inputs, one map each
        preset(
            "USDZ (AR)",
            vec![
                rgb8("diffuseColor", Channel::BaseColor),
                gray8("roughness", Channel::Roughness),
                gray8("metallic", Channel::Metallic),
                map("normal", 8, normal(VirtualMap::NormalOpenGl)),
                map("occlusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
                rgb8("emissiveColor", Channel::Emissive),
                gray8("opacity", Channel::Opacity),
            ],
        ),
    ]
}
