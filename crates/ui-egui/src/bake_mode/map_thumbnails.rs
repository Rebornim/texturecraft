use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use egui::{ColorImage, TextureHandle, TextureOptions};
use texturecraft_engine::Session;
use texturecraft_engine::core::BakedMap;

/// Side of a thumbnail's texture, in pixels (twice the row's square, for high-density screens)
const THUMBNAIL_PIXELS: u32 = 44;

/// A thumbnail's texture, with the baked data and the working size it was drawn from
type Thumbnail = (Weak<Vec<f32>>, (u32, u32), TextureHandle);

/// The Mesh Map Bakers rows' thumbnails of the baked maps, redrawn when a map is baked again
#[derive(Default)]
pub struct MapThumbnails {
    textures: BTreeMap<(usize, BakedMap), Thumbnail>,
}

impl MapThumbnails {
    /// The thumbnail of `map` as texture set `texture_set` has it baked (a running bake's finished maps included), `None` before it is baked
    pub fn texture(&mut self, context: &egui::Context, session: &Session, texture_set: usize, map: BakedMap) -> Option<TextureHandle> {
        let key = (texture_set, map);
        let baked = session.bake_preview_baked(texture_set).filter(|preview| preview.maps.contains_key(&map)).or_else(|| session.doc.baked.get(&texture_set));
        let (Some(data), Some(cache)) = (baked.and_then(|baked| baked.maps.get(&map)), session.caches.get(texture_set)) else {
            self.textures.remove(&key);
            return None;
        };

        if let Some((drawn_from, working_size, texture)) = self.textures.get(&key)
            && drawn_from.ptr_eq(&Arc::downgrade(data))
            && *working_size == cache.size
        {
            return Some(texture.clone());
        }

        let rgba = texturecraft_render::mesh_maps::mesh_map_rgba(baked, map, THUMBNAIL_PIXELS, cache.size, &cache.composite.block_scales);
        let side = THUMBNAIL_PIXELS as usize;
        let image = ColorImage::from_rgba_unmultiplied([side, side], &rgba);
        let texture = context.load_texture(format!("bake_map_{texture_set}_{}", map.id()), image, TextureOptions::LINEAR);
        self.textures.insert(key, (Arc::downgrade(data), cache.size, texture.clone()));
        Some(texture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thumbnail_is_kept_until_its_map_is_baked_again() {
        let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
        let context = egui::Context::default();
        let mut thumbnails = MapThumbnails::default();
        let map = *session.doc.baked[&0].maps.keys().next().expect("the sample has a baked map");

        let first = thumbnails.texture(&context, &session, 0, map).expect("a baked map has a thumbnail");
        assert_eq!(first.size(), [THUMBNAIL_PIXELS as usize; 2]);
        assert_eq!(thumbnails.texture(&context, &session, 0, map).unwrap().id(), first.id(), "the same bake reuses its texture");

        let baked = session.doc.baked.get_mut(&0).unwrap();
        let rebaked = baked.maps[&map].as_ref().clone();
        baked.maps.insert(map, Arc::new(rebaked));
        assert_ne!(thumbnails.texture(&context, &session, 0, map).unwrap().id(), first.id(), "a new bake is drawn again");

        session.doc.baked.get_mut(&0).unwrap().maps.remove(&map);
        assert!(thumbnails.texture(&context, &session, 0, map).is_none(), "a map not baked has none");
    }
}
