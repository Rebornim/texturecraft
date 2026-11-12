//! JNG (JPEG Network Graphics, the MNG family's still image): the colour as a baseline JPEG, the alpha kept lossless in a PNG-style zlib stream

use std::io::Write;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageError, ImageResult};

const SIGNATURE: [u8; 8] = [0x8B, b'J', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// JHDR colour types
const GRAY: u8 = 8;
const COLOR: u8 = 10;
const GRAY_ALPHA: u8 = 12;
const COLOR_ALPHA: u8 = 14;
/// Bits per JPEG sample and per alpha sample
const SAMPLE_DEPTH: u8 = 8;
/// JHDR's compression method for the JPEG colour: Huffman-coded baseline
const HUFFMAN_JPEG: u8 = 8;
/// JHDR's alpha compression method: PNG-style zlib in IDAT chunks
const ALPHA_IN_IDAT: u8 = 0;
/// The PNG row filter written before every alpha row: none
const FILTER_NONE: u8 = 0;
/// A chunk's data length is a 31-bit number
const MAX_CHUNK: usize = i32::MAX as usize;

/// Encode `bytes` (8-bit L, LA, RGB or RGBA texels of a `width`×`height` image) as JNG; `icc` goes into the JPEG stream
pub(super) fn write_jng(bytes: &[u8], (width, height): (u32, u32), color: ExtendedColorType, quality: u8, icc: Option<Vec<u8>>) -> ImageResult<Vec<u8>> {
    let (component_count, color_type, jpeg_color) = match color {
        ExtendedColorType::L8 => (1, GRAY, ExtendedColorType::L8),
        ExtendedColorType::La8 => (2, GRAY_ALPHA, ExtendedColorType::L8),
        ExtendedColorType::Rgb8 => (3, COLOR, ExtendedColorType::Rgb8),
        ExtendedColorType::Rgba8 => (4, COLOR_ALPHA, ExtendedColorType::Rgb8),
        other => return Err(unsupported(&format!("JNG holds 8-bit gray or colour, not {other:?}"))),
    };
    let has_alpha = component_count % 2 == 0;
    let color_count = if has_alpha { component_count - 1 } else { component_count };

    let color_bytes: Vec<u8> = bytes.chunks_exact(component_count).flat_map(|texel| texel.iter().take(color_count).copied()).collect();
    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, quality);
    if let Some(profile) = icc {
        // An encoder that can't embed it says no; the colours are still right
        let _ = encoder.set_icc_profile(profile);
    }
    encoder.write_image(&color_bytes, width, height, jpeg_color)?;

    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[color_type, SAMPLE_DEPTH, HUFFMAN_JPEG, 0]);
    // Alpha sample depth, compression, filter and interlace: all zero without alpha
    header.extend_from_slice(&if has_alpha { [SAMPLE_DEPTH, ALPHA_IN_IDAT, 0, 0] } else { [0; 4] });

    let mut out = SIGNATURE.to_vec();
    write_chunk(&mut out, b"JHDR", &header)?;
    write_chunk(&mut out, b"JDAT", &jpeg)?;
    if has_alpha {
        write_chunk(&mut out, b"IDAT", &alpha_stream(bytes, width, component_count)?)?;
    }
    write_chunk(&mut out, b"IEND", &[])?;
    Ok(out)
}

/// The alpha samples as a zlib stream of PNG rows, each led by its filter byte
fn alpha_stream(bytes: &[u8], width: u32, component_count: usize) -> ImageResult<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let row_length = width as usize * component_count;
    for row in bytes.chunks_exact(row_length.max(1)) {
        encoder.write_all(&[FILTER_NONE])?;
        let alpha: Vec<u8> = row.chunks_exact(component_count).filter_map(|texel| texel.last().copied()).collect();
        encoder.write_all(&alpha)?;
    }
    Ok(encoder.finish()?)
}

/// A PNG-style chunk: length, type, data, then the CRC-32 of type and data
fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> ImageResult<()> {
    if data.len() > MAX_CHUNK {
        return Err(unsupported("the image is too large for one JNG chunk"));
    }
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
    Ok(())
}

fn unsupported(message: &str) -> ImageError {
    ImageError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidInput, message.to_string()))
}
