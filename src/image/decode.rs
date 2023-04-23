use crate::utils::usize_in_mib;
use image::{
    codecs::{jpeg::JpegDecoder, png::PngDecoder},
    ColorType, GenericImageView, ImageDecoder, ImageFormat,
};
use lz4_flex::frame::{BlockSize, FrameDecoder, FrameEncoder, FrameInfo};
use std::cmp;
use std::io;
use std::time::Instant;

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
    let mut lz4_dec = FrameDecoder::new(io::Cursor::new(blob));
    let mut decompressed = Vec::with_capacity(size);
    io::copy(&mut lz4_dec, &mut decompressed)?;
    decompressed.truncate(size);
    Ok(decompressed)
}

pub fn decode_and_compress(contents: &[u8]) -> anyhow::Result<(Vec<u8>, (u32, u32))> {
    // We can stream decoding some formats although decoding may still load everything into memory
    // at once depending on how the decoder behaves
    let maybe_streamed = match image::guess_format(contents)? {
        ImageFormat::Png => {
            let dec = PngDecoder::new(io::Cursor::new(&contents))?;
            stream_decode_and_compress(dec)
        }
        ImageFormat::Jpeg => {
            let dec = JpegDecoder::new(io::Cursor::new(&contents))?;
            stream_decode_and_compress(dec)
        }
        _ => None,
    };

    match maybe_streamed {
        Some(streamed) => Ok(streamed),
        None => fallback_decode_and_compress(&contents),
    }
}

fn stream_decode_and_compress<'img, Dec>(dec: Dec) -> Option<(Vec<u8>, (u32, u32))>
where
    Dec: ImageDecoder<'img>,
{
    let total_size = dec.total_bytes();
    let dimensions = dec.dimensions();
    let start = Instant::now();

    let mut adapter = Rgba8Adapter::new(dec)?;
    lz4_compress(&mut adapter).ok().map(|lz4_blob| {
        log::debug!(
            "Streaming image decode & compression:\n\
            - Full {:.2} MiB\n\
            - Compressed {:.2} MiB\n\
            - Time {:.2?}",
            usize_in_mib(total_size as usize),
            usize_in_mib(lz4_blob.len()),
            start.elapsed(),
        );

        (lz4_blob, dimensions)
    })
}

/// An adapter that can do a streaming transformation from some pixel formats to RGBA8
enum Rgba8Adapter<'img> {
    Rgba8(Box<dyn io::Read + 'img>),
    Rgb8 {
        source: Box<dyn io::Read + 'img>,
        scratch: Vec<u8>,
    },
}

impl<'img> Rgba8Adapter<'img> {
    fn new<Dec: ImageDecoder<'img>>(dec: Dec) -> Option<Self> {
        let adapter = match dec.color_type() {
            ColorType::Rgba8 => Self::Rgba8(Box::new(dec.into_reader().ok()?)),
            ColorType::Rgb8 => Self::Rgb8 {
                source: Box::new(dec.into_reader().ok()?),
                scratch: Vec::new(),
            },
            _ => return None,
        };

        Some(adapter)
    }
}

impl<'img> io::Read for Rgba8Adapter<'img> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // TODO: can also do 16 bit adapters, but how to do them efficiently?
        match self {
            // Already the format we want, so just forward the data
            Self::Rgba8(inner) => inner.read(buf),
            // Transformation simply adds in a u8::MAX alpha channel
            // [r1, g1, b1, r2, g2, b2, ...] => [r1, g1, b1, u8::MAX, r2, g2, b2, u8::MAX, ...]
            //
            // The actual implementation
            // 1. Copies any left-over data from the scratch buffer to the output buffer
            // 2. Performs a `.read()` on the underlying source to fill the scratch buffer
            // 3. Does a pass backwards over the buffer to shift each pixel to its final position
            //    including the u8::MAX alpha channel
            // 4. Copies data from the scratch buffer to the output buffer
            // 5. Trims the scratch buffer to hold the left-over data
            //
            // This appears to be roughly just as fast as loading the full image into memory as an
            // `image::DynamicImage` and then converting `.into_rgba8()` when testing with ~55 MiB
            // of raw image data
            Self::Rgb8 { source, scratch } => {
                // Step 1.
                if scratch.len() > buf.len() {
                    buf.copy_from_slice(&scratch[..buf.len()]);
                    scratch.copy_within(buf.len().., 0);
                    scratch.truncate(scratch.len() - buf.len());
                    return Ok(buf.len());
                }

                let (left, right) = buf.split_at_mut(scratch.len());

                left.copy_from_slice(&scratch);

                // Step 2.
                let num_pixels = right.len() / 3 + 1;
                scratch.resize(num_pixels * 4, 0);
                let n = source.read(&mut scratch[..num_pixels * 3])?;
                if n == 0 {
                    scratch.clear();
                    return Ok(left.len());
                }

                // Step 3.
                let bytes_transformed = n * 4 / 3;
                let mut rgb_end = n - 1;
                let mut rgba_end = bytes_transformed - 1;
                loop {
                    scratch[rgba_end - 0] = u8::MAX;
                    scratch[rgba_end - 1] = scratch[rgb_end - 0];
                    scratch[rgba_end - 2] = scratch[rgb_end - 1];
                    scratch[rgba_end - 3] = scratch[rgb_end - 2];

                    rgba_end = match rgba_end.checked_sub(4) {
                        Some(n) => n,
                        None => break,
                    };
                    rgb_end -= 3;
                }

                // Step 4.
                right.copy_from_slice(&scratch[..right.len()]);

                // Step 5.
                scratch.copy_within(right.len().., 0);
                scratch.truncate(scratch.len() - right.len());

                Ok(left.len() + cmp::min(right.len(), bytes_transformed))
            }
        }
    }
}

fn fallback_decode_and_compress(contents: &[u8]) -> anyhow::Result<(Vec<u8>, (u32, u32))> {
    let image = image::load_from_memory(contents)?;
    let dimensions = image.dimensions();
    let image_data = image.into_rgba8().into_raw();
    log::debug!(
        "Decoded full image in memory {:.3} MiB",
        usize_in_mib(image_data.len()),
    );
    lz4_compress(&mut io::Cursor::new(image_data)).map(|lz4_blob| (lz4_blob, dimensions))
}
