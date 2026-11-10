//! The USD asset written next to exported textures: the mesh and a UsdPreviewSurface per material reading those textures

use texturecraft_core::{Channel, NormalFormat};
use texturecraft_geom::Mesh;
use texturecraft_texel::ColorSpace;

use super::usd::{SLOTS, scene, usda};
use super::usd_texture_input::UsdTextureInput;
use crate::Result;
use crate::export::{Dest, ExportOptions, PlannedFile, SourceRef, VirtualMap};

/// The preview surface inputs `planned` (one texture set's files, rendered with `options`) can feed, and which channels feed them
pub fn usd_asset_inputs(planned: &[PlannedFile], options: &ExportOptions, project_normal: NormalFormat) -> Vec<UsdTextureInput> {
    let mut inputs = Vec::new();
    for (slot, &(stem, ..)) in SLOTS.iter().enumerate() {
        let found = planned.iter().find_map(|file| {
            let map = options.preset.maps.get(file.map)?;
            let read = slot_read(stem, &map.channels, project_normal)?;
            Some((file, read))
        });
        let Some((file, (output, directx))) = found else { continue };

        let color = matches!(stem, "BaseColor" | "Emissive");
        let srgb = color && !file.format.is_float() && options.space_for(file.format) == ColorSpace::Srgb;
        let directx_normal = options.normal_format.map_or(directx, |format| format == NormalFormat::DirectX);
        inputs.push(UsdTextureInput { slot, file: file.name.clone(), output, srgb, directx_normal: stem == "Normal" && directx_normal });
    }

    inputs
}

/// Where in a map's channels the input `stem` sits: the texture output to read, and whether a normal is DirectX
fn slot_read(stem: &str, channels: &[(Dest, SourceRef)], project_normal: NormalFormat) -> Option<(&'static str, bool)> {
    let single = |channel: Channel| {
        channels.iter().find_map(|(dest, source)| match source {
            SourceRef::Channel { channel: c, .. } if *c == channel => Some((output_of(*dest), false)),
            _ => None,
        })
    };
    // A colour or normal input needs its three components in R, G and B
    let rgb = |is_source: &dyn Fn(&SourceRef) -> bool| {
        let in_rgb = [Dest::R, Dest::G, Dest::B].iter().all(|wanted| channels.iter().any(|(dest, source)| dest == wanted && is_source(source)));
        in_rgb.then_some(("rgb", false))
    };

    match stem {
        "BaseColor" => rgb(&|source| matches!(source, SourceRef::Channel { channel: Channel::BaseColor, .. })),
        "Emissive" => rgb(&|source| matches!(source, SourceRef::Channel { channel: Channel::Emissive, .. })),
        "Normal" => {
            let directx = channels.iter().find_map(|(_, source)| match source {
                SourceRef::Virtual { map: VirtualMap::NormalDirectX, .. } => Some(true),
                SourceRef::Virtual { map: VirtualMap::NormalOpenGl, .. } => Some(false),
                SourceRef::Virtual { map: VirtualMap::HeightNormal, .. } | SourceRef::Channel { channel: Channel::Normal, .. } => {
                    Some(project_normal == NormalFormat::DirectX)
                }
                _ => None,
            })?;
            let is_normal = |source: &SourceRef| {
                matches!(
                    source,
                    SourceRef::Virtual { map: VirtualMap::NormalOpenGl | VirtualMap::NormalDirectX | VirtualMap::HeightNormal, .. }
                        | SourceRef::Channel { channel: Channel::Normal, .. }
                )
            };
            rgb(&is_normal).map(|(output, _)| (output, directx))
        }
        "Roughness" => single(Channel::Roughness),
        "Metallic" => single(Channel::Metallic),
        "Opacity" => single(Channel::Opacity),
        "Clearcoat" => single(Channel::CoatWeight),
        "ClearcoatRoughness" => single(Channel::CoatRoughness),
        _ => None,
    }
}

/// The texture output that reads destination `dest`
fn output_of(dest: Dest) -> &'static str {
    match dest {
        Dest::G => "g",
        Dest::B => "b",
        Dest::A => "a",
        Dest::R | Dest::L => "r",
    }
}

/// The mesh as a usda layer named `name`, material `m` reading `inputs[m]` (files next to the layer)
pub fn usd_asset(mesh: &Mesh, name: &str, inputs: &[Vec<UsdTextureInput>]) -> Result<String> {
    Ok(usda(&scene(mesh, name, inputs)?))
}
