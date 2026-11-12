//! PSD writer, from Adobe's published file format: each output map a layer, a `/` in its name a layer group

mod psd_entry;
mod psd_layer;

pub use psd_layer::PsdLayer;

use psd_entry::PsdEntry;

const SIGNATURE: &[u8; 4] = b"8BPS";
const VERSION: u16 = 1;
const RGB_MODE: u16 = 3;
const RGB_CHANNELS: u16 = 3;
const BLOCK_SIGNATURE: &[u8; 4] = b"8BIM";
const NORMAL_BLEND: &[u8; 4] = b"norm";
const PASS_THROUGH_BLEND: &[u8; 4] = b"pass";
/// Channel ids of a layer record
const ALPHA_CHANNEL: i16 = -1;
const COLOR_CHANNELS: [i16; 3] = [0, 1, 2];
const OPAQUE: u8 = 255;
/// Image data compression: none, or PackBits run-length encoding per row
const RAW: u16 = 0;
const PACKBITS: u16 = 1;
/// The longest PackBits run or literal
const PACKBITS_MAX: usize = 128;
/// Section divider types ('lsct'): an open folder, and the hidden marker closing a group
const OPEN_FOLDER: u32 = 1;
const GROUP_END_MARKER: u32 = 3;
const GROUP_END_NAME: &str = "</Layer group>";
/// The longest layer name a Pascal string holds; the full name goes in 'luni'
const MAX_PASCAL_NAME: usize = 255;
/// PSD's largest image side
pub const MAX_PSD_SIDE: u32 = 30_000;

/// Encode `layers` (top of the stack first, each `width`×`height`) as an 8 or 16-bit RGB PSD
pub fn write_psd((width, height): (u32, u32), depth: u8, layers: &[PsdLayer]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || width > MAX_PSD_SIDE || height > MAX_PSD_SIDE {
        return Err(format!("a PSD is 1 to {MAX_PSD_SIDE} texels a side, not {width}×{height}"));
    }
    if !matches!(depth, 8 | 16) {
        return Err(format!("a PSD is written with 8 or 16 bits, not {depth}"));
    }
    let texels = width as usize * height as usize;
    if let Some(layer) = layers.iter().find(|layer| !(1..=4).contains(&layer.component_count) || layer.samples.len() != texels * layer.component_count) {
        return Err(format!("layer \"{}\" doesn't match the document's {width}×{height}", layer.path.join("/")));
    }
    let writer = Writer { width, height, depth };

    let mut out = Vec::new();
    out.extend_from_slice(SIGNATURE);
    put_u16(&mut out, VERSION);
    out.extend_from_slice(&[0; 6]);
    put_u16(&mut out, RGB_CHANNELS);
    put_u32(&mut out, height);
    put_u32(&mut out, width);
    put_u16(&mut out, u16::from(depth));
    put_u16(&mut out, RGB_MODE);
    // No colour mode data, no image resources
    put_u32(&mut out, 0);
    put_u32(&mut out, 0);

    let layer_info = writer.layer_info(layers)?;
    let mut section = Vec::new();
    if depth == 8 {
        put_length(&mut section, layer_info.len())?;
        section.extend_from_slice(&layer_info);
        // No global layer mask
        put_u32(&mut section, 0);
    } else {
        // Photoshop keeps a 16-bit document's layers in an 'Lr16' block after an empty layer info
        put_u32(&mut section, 0);
        put_u32(&mut section, 0);
        section.extend_from_slice(BLOCK_SIGNATURE);
        section.extend_from_slice(b"Lr16");
        put_length(&mut section, layer_info.len())?;
        section.extend_from_slice(&layer_info);
    }
    put_length(&mut out, section.len())?;
    out.extend_from_slice(&section);

    writer.composite(&mut out, layers);
    Ok(out)
}

/// The document's size and depth, for the parts that depend on them
struct Writer {
    width: u32,
    height: u32,
    depth: u8,
}

impl Writer {
    fn max(&self) -> u16 {
        if self.depth == 8 { u16::from(u8::MAX) } else { u16::MAX }
    }

    /// The layer count, the records bottom of the stack first, then each record's channel data; padded to 4 bytes
    fn layer_info(&self, layers: &[PsdLayer]) -> Result<Vec<u8>, String> {
        let mut entries = entries(layers);
        entries.reverse();
        let count = i16::try_from(entries.len()).map_err(|_| format!("a PSD holds at most {} layers and groups", i16::MAX))?;

        let mut records = Vec::new();
        let mut image_data = Vec::new();
        for entry in &entries {
            let channels = match entry {
                PsdEntry::Layer(layer) => self.layer_channels(layer),
                PsdEntry::GroupStart(_) | PsdEntry::GroupEnd => empty_channels(),
            };
            let (top_left, bottom_right) = match entry {
                PsdEntry::Layer(_) => ((0, 0), (self.height, self.width)),
                PsdEntry::GroupStart(_) | PsdEntry::GroupEnd => ((0, 0), (0, 0)),
            };
            for value in [top_left.0, top_left.1, bottom_right.0, bottom_right.1] {
                put_u32(&mut records, value);
            }
            put_u16(&mut records, channels.len() as u16);
            for (id, data) in &channels {
                put_u16(&mut records, *id as u16);
                put_length(&mut records, data.len())?;
            }
            records.extend_from_slice(BLOCK_SIGNATURE);
            records.extend_from_slice(NORMAL_BLEND);
            // Opacity, clipping (base), flags (visible), filler
            records.extend_from_slice(&[OPAQUE, 0, 0, 0]);

            let (name, divider) = match entry {
                PsdEntry::Layer(layer) => (layer.path.last().map_or("", String::as_str), None),
                PsdEntry::GroupStart(name) => (*name, Some(OPEN_FOLDER)),
                PsdEntry::GroupEnd => (GROUP_END_NAME, Some(GROUP_END_MARKER)),
            };
            let extra = extra_data(name, divider);
            put_length(&mut records, extra.len())?;
            records.extend_from_slice(&extra);

            for (_, data) in channels {
                image_data.extend_from_slice(&data);
            }
        }

        let mut info = Vec::with_capacity(2 + records.len() + image_data.len() + 3);
        put_u16(&mut info, count as u16);
        info.extend_from_slice(&records);
        info.extend_from_slice(&image_data);
        info.resize(info.len().next_multiple_of(4), 0);
        Ok(info)
    }

    /// A pixel layer's channels with their compressed data: alpha (when it has one), then red, green and blue
    fn layer_channels(&self, layer: &PsdLayer) -> Vec<(i16, Vec<u8>)> {
        let texels = self.width as usize * self.height as usize;
        let mut channels = Vec::with_capacity(4);
        if layer.has_alpha() {
            let max = self.max();
            channels.push((ALPHA_CHANNEL, self.packed_plane((0..texels).map(|texel| layer.alpha(texel, max)))));
        }
        for (color, id) in COLOR_CHANNELS.into_iter().enumerate() {
            channels.push((id, self.packed_plane((0..texels).map(|texel| layer.color(texel, color)))));
        }
        channels
    }

    /// One channel's samples, PackBits-compressed per row: the compression, the row lengths, the rows
    fn packed_plane(&self, samples: impl Iterator<Item = u16>) -> Vec<u8> {
        let mut out = Vec::new();
        put_u16(&mut out, PACKBITS);
        let (lengths, rows) = self.pack_rows(samples, self.height as usize);
        out.extend_from_slice(&lengths);
        out.extend_from_slice(&rows);
        out
    }

    /// `row_count` rows of samples PackBits-compressed: the u16 row lengths, and the packed rows
    fn pack_rows(&self, samples: impl Iterator<Item = u16>, row_count: usize) -> (Vec<u8>, Vec<u8>) {
        let row_bytes = self.width as usize * usize::from(self.depth / 8);
        let mut bytes = Vec::with_capacity(row_bytes * row_count);
        for sample in samples {
            if self.depth == 8 {
                bytes.push(sample.min(u16::from(u8::MAX)) as u8);
            } else {
                bytes.extend_from_slice(&sample.to_be_bytes());
            }
        }
        let mut lengths = Vec::with_capacity(row_count * 2);
        let mut packed = Vec::with_capacity(bytes.len() / 2);
        for row in bytes.chunks(row_bytes.max(1)) {
            let start = packed.len();
            packbits(row, &mut packed);
            // A packed row is at most a byte per 128 longer than the 16-bit row of a 30000-texel side
            put_u16(&mut lengths, (packed.len() - start) as u16);
        }
        (lengths, packed)
    }

    /// The flattened image other readers show: the layers over white, each in turn, PackBits-compressed
    fn composite(&self, out: &mut Vec<u8>, layers: &[PsdLayer]) {
        let texels = self.width as usize * self.height as usize;
        let max = f32::from(self.max());
        let mut flat = vec![max; texels * 3];
        for layer in layers.iter().rev() {
            for (texel, rgb) in flat.chunks_exact_mut(3).enumerate() {
                let alpha = f32::from(layer.alpha(texel, self.max())) / max;
                for (color, value) in rgb.iter_mut().enumerate() {
                    *value = f32::from(layer.color(texel, color)) * alpha + *value * (1.0 - alpha);
                }
            }
        }
        put_u16(out, PACKBITS);
        let planes = (0..3).flat_map(|color| flat.iter().skip(color).step_by(3).map(|value| (value + 0.5) as u16).collect::<Vec<_>>());
        let (lengths, rows) = self.pack_rows(planes, self.height as usize * 3);
        out.extend_from_slice(&lengths);
        out.extend_from_slice(&rows);
    }
}

/// The layer list top first, a group opening before its first layer and closing after its last
fn entries(layers: &[PsdLayer]) -> Vec<PsdEntry<'_>> {
    let mut entries = Vec::new();
    let mut open: Vec<&str> = Vec::new();
    for layer in layers {
        let folders: Vec<&str> = layer.path.iter().take(layer.path.len().saturating_sub(1)).map(String::as_str).collect();
        let shared = open.iter().zip(&folders).take_while(|(a, b)| a == b).count();
        for _ in shared..open.len() {
            entries.push(PsdEntry::GroupEnd);
        }
        open.truncate(shared);
        for folder in folders.iter().skip(shared) {
            entries.push(PsdEntry::GroupStart(folder));
            open.push(folder);
        }
        entries.push(PsdEntry::Layer(layer));
    }
    entries.extend(open.iter().map(|_| PsdEntry::GroupEnd));
    entries
}

/// A group record's channels: no texels, so only each one's compression
fn empty_channels() -> Vec<(i16, Vec<u8>)> {
    std::iter::once(ALPHA_CHANNEL).chain(COLOR_CHANNELS).map(|id| (id, RAW.to_be_bytes().to_vec())).collect()
}

/// A layer record's extra data: no mask, no blending ranges, the name, then its Unicode name and group role
fn extra_data(name: &str, divider: Option<u32>) -> Vec<u8> {
    let mut extra = Vec::new();
    put_u32(&mut extra, 0);
    put_u32(&mut extra, 0);
    let ascii: Vec<u8> = name.chars().map(|c| if c.is_ascii() && !c.is_ascii_control() { c as u8 } else { b'_' }).take(MAX_PASCAL_NAME).collect();
    // The length byte and the name, padded to a multiple of 4
    let name_start = extra.len();
    extra.push(ascii.len() as u8);
    extra.extend_from_slice(&ascii);
    extra.resize(name_start + (1 + ascii.len()).next_multiple_of(4), 0);

    let utf16: Vec<u16> = name.encode_utf16().collect();
    let mut unicode = Vec::with_capacity(4 + utf16.len() * 2);
    put_u32(&mut unicode, utf16.len() as u32);
    for unit in utf16 {
        put_u16(&mut unicode, unit);
    }
    tagged_block(&mut extra, b"luni", &unicode);

    if let Some(kind) = divider {
        let mut section = Vec::with_capacity(12);
        put_u32(&mut section, kind);
        section.extend_from_slice(BLOCK_SIGNATURE);
        section.extend_from_slice(PASS_THROUGH_BLEND);
        tagged_block(&mut extra, b"lsct", &section);
    }
    extra
}

/// '8BIM', the key, the length, then the data padded to an even length
fn tagged_block(out: &mut Vec<u8>, key: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(BLOCK_SIGNATURE);
    out.extend_from_slice(key);
    let padded = data.len().next_multiple_of(2);
    put_u32(out, padded as u32);
    out.extend_from_slice(data);
    out.resize(out.len() + padded - data.len(), 0);
}

/// PackBits: runs of 2 to 128 equal bytes as (1 − n, byte), everything else as literals (n − 1, n bytes)
fn packbits(row: &[u8], out: &mut Vec<u8>) {
    let mut start = 0;
    while let Some(&first) = row.get(start) {
        let run = row.iter().skip(start).take(PACKBITS_MAX).take_while(|&&byte| byte == first).count();
        if run >= 2 {
            out.push((1 - run as i16) as u8);
            out.push(first);
            start += run;
            continue;
        }
        let mut end = start + 1;
        while end < row.len() && end - start < PACKBITS_MAX && row.get(end) != row.get(end + 1) {
            end += 1;
        }
        out.push((end - start - 1) as u8);
        out.extend_from_slice(row.get(start..end).unwrap_or_default());
        start = end;
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// A section or channel length, which PSD keeps in 32 bits
fn put_length(out: &mut Vec<u8>, length: usize) -> Result<(), String> {
    let length = u32::try_from(length).map_err(|_| "the layers are too large for a PSD (over 4 GB)".to_string())?;
    put_u32(out, length);
    Ok(())
}
