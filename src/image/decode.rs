use std::io;
use std::time::Instant;

use crate::metrics::{histogram, HistTag};
use crate::utils::usize_in_mib;

use image::GenericImageView;
use lz4_flex::frame::{BlockSize, FrameDecoder, FrameEncoder, FrameInfo};

pub fn lz4_compress<R: io::Read>(reader: &mut R) -> anyhow::Result<Vec<u8>> {
    let mut frame_info = FrameInfo::new();
    frame_info.block_size = BlockSize::Max256KB;
    let mut lz4_enc = FrameEncoder::with_frame_info(frame_info, Vec::with_capacity(8 * 1_024));

    io::copy(reader, &mut lz4_enc)?;
    let mut lz4_blob = lz4_enc.finish()?;
    lz4_blob.shrink_to_fit();

    Ok(lz4_blob)
}

pub fn lz4_decompress(blob: &[u8], size: usize) -> anyhow::Result<Vec<u8>> {
    let start = Instant::now();
    let mut lz4_dec = FrameDecoder::new(io::Cursor::new(blob));
    let mut decompressed = Vec::with_capacity(size);
    io::copy(&mut lz4_dec, &mut decompressed)?;
    decompressed.truncate(size);
    histogram!(HistTag::ImageDecompress).record(start.elapsed());
    Ok(decompressed)
}

pub type ImageParts = (Vec<u8>, (u32, u32));

pub fn decode_and_compress(contents: &[u8]) -> anyhow::Result<ImageParts> {
    let image = image::load_from_memory(contents)?;
    let dimensions = image.dimensions();
    let image_data = image.into_rgba8().into_raw();
    tracing::debug!(
        "Decoded full image in memory {:.3} MiB",
        usize_in_mib(image_data.len()),
    );
    lz4_compress(&mut io::Cursor::new(image_data)).map(|lz4_blob| (lz4_blob, dimensions))
}
