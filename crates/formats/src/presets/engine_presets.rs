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

/// Generic metallic/roughness maps, one per input
fn pbr_metallic_roughness() -> Vec<MapSpec> {
    vec![
        map("BaseColor", 8, base_with_alpha()),
        gray8("Metallic", Channel::Metallic),
        gray8("Roughness", Channel::Roughness),
        map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
        height("Height"),
        rgb8("Emissive", Channel::Emissive),
        map("AmbientOcclusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
    ]
}

/// V-Ray, metalness: base color as diffuse, roughness and metalness (the material's reflection stays white)
fn vray_metal_roughness() -> Vec<MapSpec> {
    vec![rgb8("diffuse", Channel::BaseColor), gray8("roughness", Channel::Roughness), gray8("metalness", Channel::Metallic), project_normal("normal")]
}

/// The older UDIM layout, each texture set one UV tile named by its number: `$mesh_<map>.$textureSet`
fn udim_legacy(maps: Vec<MapSpec>) -> Vec<MapSpec> {
    maps.into_iter()
        .map(|mut spec| {
            if let Some(suffix) = spec.file.strip_prefix("$mesh_$textureSet_") {
                spec.file = format!("$mesh_{suffix}.$textureSet");
            }
            spec
        })
        .collect()
}

pub(super) fn engine_presets() -> Vec<ExportPreset> {
    let preset = |name: &str, maps: Vec<MapSpec>| ExportPreset { name: name.into(), maps };
    vec![
        preset("PBR Metallic Roughness", pbr_metallic_roughness()),
        // The same maps in a folder per mesh, which a PSD export turns into a layer group
        preset(
            "PBR Metallic Roughness (folders or PSD groups)",
            pbr_metallic_roughness()
                .into_iter()
                .map(|mut spec| {
                    spec.file = spec.file.replacen("$mesh_", "$mesh/", 1);
                    spec
                })
                .collect(),
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
        preset("V-Ray (Metal Roughness)", vray_metal_roughness()),
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
        // Khronos' specular-glossiness layout from the document's own channels: diffuse with opacity, specular with glossiness in alpha
        preset(
            "PBR Specular Glossiness",
            vec![
                map("Diffuse", 8, base_with_alpha()),
                map("SpecularGlossiness", 8, rgb(|k| ch(Channel::Specular, k)).into_iter().chain([(Dest::A, ch(Channel::Glossiness, 0))]).collect()),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                map("Occlusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
                rgb8("Emissive", Channel::Emissive),
            ],
        ),
        // The same converted from metallic/roughness: metal blackens the diffuse and colours the specular
        preset(
            "PBR Specular Glossiness (converted from Metallic Roughness)",
            vec![
                map("Diffuse", 8, rgb(|k| virt(VirtualMap::Diffuse, k)).into_iter().chain([(Dest::A, ch(Channel::Opacity, 0))]).collect()),
                map(
                    "SpecularGlossiness",
                    8,
                    rgb(|k| virt(VirtualMap::SpecularColor, k)).into_iter().chain([(Dest::A, virt(VirtualMap::Glossiness, 0))]).collect(),
                ),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                map("Occlusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
                rgb8("Emissive", Channel::Emissive),
            ],
        ),
        // Older, non-energy-conserving renderers: diffuse, specular and glossiness each a file of their own
        preset(
            "Non-PBR Specular Glossiness",
            vec![
                rgb8("Diffuse", Channel::BaseColor),
                rgb8("Specular", Channel::Specular),
                gray8("Glossiness", Channel::Glossiness),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                height("Height"),
                rgb8("Emissive", Channel::Emissive),
                gray8("Opacity", Channel::Opacity),
            ],
        ),
        // Corona Physical Material in its default roughness mode
        preset(
            "Corona (Physical)",
            vec![
                rgb8("basecolor", Channel::BaseColor),
                gray8("metalness", Channel::Metallic),
                gray8("roughness", Channel::Roughness),
                project_normal("normal"),
                gray8("opacity", Channel::Opacity),
                height("height"),
            ],
        ),
        // KeyShot's classic maps: diffuse, specular, a normal map loaded as bump, opacity
        preset(
            "KeyShot",
            vec![
                converted8("diffuse", VirtualMap::Diffuse),
                converted8("specular", VirtualMap::SpecularColor),
                project_normal("normal"),
                gray8("opacity", Channel::Opacity),
            ],
        ),
        // KeyShot's Generic material (metallic workflow)
        preset(
            "KeyShot (Generic)",
            vec![
                rgb8("basecolor", Channel::BaseColor),
                gray8("metallic", Channel::Metallic),
                gray8("roughness", Channel::Roughness),
                project_normal("normal"),
                gray8("opacity", Channel::Opacity),
            ],
        ),
        // Redshift Standard Material; Redshift reads OpenGL normal maps
        preset(
            "Redshift (Standard)",
            vec![
                rgb8("basecolor", Channel::BaseColor),
                gray8("metalness", Channel::Metallic),
                gray8("roughness", Channel::Roughness),
                map("normal", 8, normal(VirtualMap::NormalOpenGl)),
                rgb8("emissive", Channel::Emissive),
                gray8("opacity", Channel::Opacity),
                gray8("sss", Channel::ScatterWeight),
                gray8("coat", Channel::CoatWeight),
                gray8("sheen", Channel::SheenWeight),
            ],
        ),
        // RenderMan PxrDisney inputs
        preset(
            "RenderMan (PxrDisney)",
            vec![
                rgb8("baseColor", Channel::BaseColor),
                gray8("metallic", Channel::Metallic),
                gray8("roughness", Channel::Roughness),
                gray8("specular", Channel::SpecularLevel),
                gray8("subsurface", Channel::ScatterWeight),
                gray8("sheen", Channel::SheenWeight),
                gray8("clearcoat", Channel::CoatWeight),
                rgb8("emitColor", Channel::Emissive),
                gray8("presence", Channel::Opacity),
                project_normal("normal"),
            ],
        ),
        // RenderMan PxrSurface through its metallic workflow node
        preset(
            "RenderMan (PxrSurface)",
            vec![
                rgb8("baseColor", Channel::BaseColor),
                gray8("metallic", Channel::Metallic),
                gray8("specularRoughness", Channel::Roughness),
                gray8("presence", Channel::Opacity),
                rgb8("glowColor", Channel::Emissive),
                project_normal("normal"),
            ],
        ),
        // Roblox SurfaceAppearance: colour with opacity in alpha, OpenGL normal (all Roblox reads), suffixes its importer assigns
        preset(
            "Roblox (SurfaceAppearance)",
            vec![
                map("Color", 8, base_with_alpha()),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                gray8("Metalness", Channel::Metallic),
                gray8("Roughness", Channel::Roughness),
            ],
        ),
        // Roblox MaterialVariant: the same maps, its colour without alpha (MaterialVariant doesn't use it)
        preset(
            "Roblox (MaterialVariant)",
            vec![
                rgb8("Color", Channel::BaseColor),
                map("Normal", 8, normal(VirtualMap::NormalOpenGl)),
                gray8("Metalness", Channel::Metallic),
                gray8("Roughness", Channel::Roughness),
            ],
        ),
        // Sketchfab assigns each file to a slot by its suffix and reads normals as DirectX by default
        preset(
            "Sketchfab (PBR Metal Roughness)",
            vec![
                map("BaseColor", 8, base_with_alpha()),
                map("Normal", 8, normal(VirtualMap::NormalDirectX)),
                gray8("Metallic", Channel::Metallic),
                gray8("Roughness", Channel::Roughness),
                map("AmbientOcclusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
                rgb8("Emissive", Channel::Emissive),
            ],
        ),
        // Dota 2 hero shader (OpenGL normals): masks1 detail, fresnel, metalness, self-illumination; masks2 specular, rim, tint, exponent
        preset(
            "Dota 2",
            vec![
                map("color", 8, base_with_alpha()),
                map("normal", 8, normal(VirtualMap::NormalOpenGl)),
                map(
                    "masks1",
                    8,
                    vec![
                        (Dest::R, SourceRef::Constant(0.0)),
                        (Dest::G, SourceRef::Constant(0.0)),
                        (Dest::B, ch(Channel::Metallic, 0)),
                        (Dest::A, SourceRef::Constant(0.0)),
                    ],
                ),
                map(
                    "masks2",
                    8,
                    vec![
                        (Dest::R, ch(Channel::SpecularLevel, 0)),
                        (Dest::G, SourceRef::Constant(0.0)),
                        (Dest::B, ch(Channel::Metallic, 0)),
                        (Dest::A, virt(VirtualMap::Glossiness, 0)),
                    ],
                ),
            ],
        ),
        // Spark AR Studio's physically based material: base colour with opacity, glTF-style ORM, normal, emissive
        preset(
            "Spark AR Studio",
            vec![
                map("baseColor", 8, base_with_alpha()),
                map("ORM", 8, orm()),
                map("normal", 8, normal(VirtualMap::NormalOpenGl)),
                rgb8("emissive", Channel::Emissive),
            ],
        ),
        // Maxwell Render matches maps by file name; its metallic/roughness material
        preset(
            "Maxwell (Metallic Roughness)",
            vec![
                rgb8("BaseColor", Channel::BaseColor),
                gray8("Metallic", Channel::Metallic),
                gray8("Roughness", Channel::Roughness),
                project_normal("Normal"),
                height("Height"),
                rgb8("Emissive", Channel::Emissive),
                gray8("Opacity", Channel::Opacity),
            ],
        ),
        // Maxwell Render's specular/glossiness material, converted from metallic/roughness
        preset(
            "Maxwell (Specular Glossiness)",
            vec![
                converted8("Diffuse", VirtualMap::Diffuse),
                converted8("Specular", VirtualMap::SpecularColor),
                map("Glossiness", 8, gray(virt(VirtualMap::Glossiness, 0))),
                project_normal("Normal"),
                height("Height"),
                rgb8("Emissive", Channel::Emissive),
                gray8("Opacity", Channel::Opacity),
            ],
        ),
        // Shade 3D's PBR material: a map per metallic/roughness input
        preset(
            "Shade 3D",
            vec![
                rgb8("BaseColor", Channel::BaseColor),
                gray8("Metallic", Channel::Metallic),
                gray8("Roughness", Channel::Roughness),
                project_normal("Normal"),
                map("AmbientOcclusion", 8, gray(virt(VirtualMap::AoMixed, 0))),
                height("Height"),
                rgb8("Emissive", Channel::Emissive),
                gray8("Opacity", Channel::Opacity),
            ],
        ),
        // Arnold's older aiStandard shader (diffuse and specular colours, no metalness), in the older UDIM layout
        preset(
            "Arnold (aiStandard, UDIM legacy)",
            udim_legacy(vec![
                converted8("diffuse", VirtualMap::Diffuse),
                converted8("specular", VirtualMap::SpecularColor),
                gray8("specular_roughness", Channel::Roughness),
                map("normal", 8, normal(VirtualMap::NormalOpenGl)),
                rgb8("emission", Channel::Emissive),
                gray8("opacity", Channel::Opacity),
            ]),
        ),
        preset("V-Ray (Metal Roughness, UDIM legacy)", udim_legacy(vray_metal_roughness())),
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
