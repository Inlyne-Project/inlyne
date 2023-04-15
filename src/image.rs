use crate::positioner::DEFAULT_MARGIN;
use crate::utils::{usize_in_mib, Align, Point, Size};
use crate::InlyneEvent;
use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use image::{
    codecs::{jpeg::JpegDecoder, png::PngDecoder},
    ColorType, GenericImageView, ImageBuffer, ImageDecoder, ImageFormat, RgbaImage,
};
use lz4_flex::frame::{BlockSize, FrameDecoder, FrameEncoder, FrameInfo};
use std::cell::Cell;
use std::cmp;
use std::io::{self, Cursor, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use usvg::{TreeParsing, TreeTextToPath};
use wgpu::util::DeviceExt;
use wgpu::{BindGroup, Device, TextureFormat};
use winit::event_loop::EventLoopProxy;

use std::borrow::Cow;

// TODO: create test images that are at least 3_000 or so pixels, so that it actually tests the
// reads buffering

#[derive(Debug, Clone)]
pub enum ImageSize {
    PxWidth(u32),
    PxHeight(u32),
}

fn decode_and_compress(contents: &[u8]) -> anyhow::Result<(Vec<u8>, (u32, u32))> {
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
        None => {
            println!("Falling back to full decode");
            fallback_decode_and_compress(&contents)
        }
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
    println!(
        "Decoded full image in memory {:.3} MiB",
        image_data.len() as f32 / 1_024.0 / 1_024.0
    );
    lz4_compress(&mut io::Cursor::new(image_data)).map(|lz4_blob| (lz4_blob, dimensions))
}

fn lz4_compress<R: io::Read>(reader: &mut R) -> anyhow::Result<Vec<u8>> {
    let mut frame_info = FrameInfo::new();
    frame_info.block_size = BlockSize::Max256KB;
    let mut lz4_enc = FrameEncoder::with_frame_info(frame_info, Vec::with_capacity(8 * 1_024));

    io::copy(reader, &mut lz4_enc)?;
    let mut lz4_blob = lz4_enc.finish()?;
    lz4_blob.shrink_to_fit();

    Ok(lz4_blob)
}

#[derive(Debug, Default, Clone)]
pub struct ImageData {
    lz4_blob: Vec<u8>,
    scale: bool,
    dimensions: (u32, u32),
}

// TODO: create a writer for the byte data? Probably leave that for another PR

impl ImageData {
    fn load(bytes: &[u8], scale: bool) -> anyhow::Result<Self> {
        let (lz4_blob, dimensions) = decode_and_compress(bytes)?;
        Ok(Self {
            lz4_blob,
            scale,
            dimensions,
        })
    }

    fn new(image: RgbaImage, scale: bool) -> Self {
        let dimensions = image.dimensions();

        let start = Instant::now();
        let mut frame_info = FrameInfo::new();
        // This seems to speed up decompressing considerably
        frame_info.block_size = BlockSize::Max256KB;
        let mut lz4_enc = FrameEncoder::with_frame_info(frame_info, Vec::with_capacity(8_192));
        lz4_enc.write_all(image.as_raw()).expect("I/O is in memory");
        let mut lz4_blob = lz4_enc.finish().expect("We control compression");
        lz4_blob.shrink_to_fit();
        // TODO: update this log message
        log::debug!(
            "Compressing image: Full {:.2} MiB - Compressed {:.2} MiB - Time {:.2?}",
            usize_in_mib(image.as_raw().len()),
            usize_in_mib(lz4_blob.len()),
            start.elapsed(),
        );

        Self {
            dimensions,
            lz4_blob,
            scale,
        }
    }

    fn rgba_image_byte_size(&self) -> usize {
        let (x, y) = self.dimensions;
        x as usize * y as usize * 4
    }
}

pub enum DataStage {
    Processing((Arc<Runtime>, JoinHandle<anyhow::Result<Arc<ImageData>>>)),
    Finished(Arc<ImageData>),
}

impl Default for DataStage {
    fn default() -> Self {
        Self::Finished(Default::default())
    }
}

#[derive(Default)]
pub struct Image {
    pub data: Cell<DataStage>,
    pub is_aligned: Option<Align>,
    pub size: Option<ImageSize>,
    pub bind_group: Option<Arc<wgpu::BindGroup>>,
    pub is_link: Option<String>,
    pub hidpi_scale: f32,
}

impl Image {
    pub fn get_data(&self) -> anyhow::Result<Arc<ImageData>> {
        match self.data.take() {
            DataStage::Finished(data) => {
                self.data.set(DataStage::Finished(data.clone()));
                Ok(data)
            }
            DataStage::Processing((rt, data_future)) => {
                let data = rt.block_on(data_future)??;
                self.data.set(DataStage::Finished(data.clone()));
                Ok(data)
            }
        }
    }

    pub fn try_get_data(&mut self) -> Option<anyhow::Result<Arc<ImageData>>> {
        match *self.data.get_mut() {
            DataStage::Finished(_) => Some(self.get_data()),
            DataStage::Processing((_, ref future)) => {
                if future.is_finished() {
                    Some(self.get_data())
                } else {
                    None
                }
            }
        }
    }

    pub fn create_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        bindgroup_layout: &wgpu::BindGroupLayout,
    ) -> anyhow::Result<Arc<BindGroup>> {
        let image = self.get_data()?;
        let dimensions = self
            .buffer_dimensions()
            .context("Could not get buffer dimensions")?;
        if dimensions.0 == 0 || dimensions.1 == 0 {
            return Err(anyhow::Error::msg("Invalid buffer dimensions"));
        }
        let start = Instant::now();
        let mut lz4_dec = FrameDecoder::new(Cursor::new(&image.lz4_blob));
        let mut rgba_image = Vec::with_capacity(image.rgba_image_byte_size());
        io::copy(&mut lz4_dec, &mut rgba_image).expect("I/O is in memory");

        log::debug!("Decompressing image: Time {:.2?}", start.elapsed());

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("Image Texture"),
            view_formats: &[],
        });
        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            &rgba_image,
            // The layout of the texture
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(4 * dimensions.0),
                rows_per_image: std::num::NonZeroU32::new(dimensions.1),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bindgroup_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
            label: Some("Image Bind Group"),
        });
        let bind_group = Arc::new(bind_group);
        self.bind_group = Some(bind_group.clone());
        Ok(bind_group)
    }

    pub fn from_src(
        src: String,
        file_path: PathBuf,
        hidpi_scale: f32,
        event_proxy: EventLoopProxy<InlyneEvent>,
        rt: Arc<Runtime>,
    ) -> anyhow::Result<Image> {
        let image_data = async move {
            let mut src_path = PathBuf::from(&src);
            if src_path.is_relative() {
                if let Some(parent_dir) = file_path.parent() {
                    src_path = parent_dir.join(src_path.strip_prefix("./").unwrap_or(&src_path));
                }
            }

            let image_data = if let Ok(img_file) = tokio::fs::read(&src_path).await {
                img_file
            } else {
                reqwest::get(&src).await?.bytes().await?.to_vec()
            };

            let image = if let Ok(image) = ImageData::load(&image_data, true) {
                Ok(Arc::new(image))
            } else {
                let opt = usvg::Options::default();
                let mut rtree = usvg::Tree::from_data(&image_data, &opt)?;
                let mut fontdb = usvg::fontdb::Database::new();
                fontdb.load_system_fonts();
                rtree.convert_text(&fontdb);
                let pixmap_size = rtree.size.to_screen_size();
                let mut pixmap = tiny_skia::Pixmap::new(
                    (pixmap_size.width() as f32 * hidpi_scale) as u32,
                    (pixmap_size.height() as f32 * hidpi_scale) as u32,
                )
                .context("Couldn't create svg pixmap")?;
                resvg::render(
                    &rtree,
                    resvg::FitTo::Zoom(hidpi_scale),
                    tiny_skia::Transform::default(),
                    pixmap.as_mut(),
                )
                .context("Svg failed to render")?;
                Ok(Arc::new(ImageData::new(
                    ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.data().into())
                        .context("Svg buffer has invalid dimensions")?,
                    false,
                )))
            };
            event_proxy.send_event(InlyneEvent::Reposition).unwrap();
            image
        };
        let image = Image {
            data: Cell::new(DataStage::Processing((rt.clone(), rt.spawn(image_data)))),
            hidpi_scale,
            ..Default::default()
        };

        // Load image in background
        //let image_ref = image.data.clone();
        /*
        rt.spawn(async move {
            image_ref.get().await;
            event_proxy.send_event(InlyneEvent::Reposition).unwrap();
        });
        */

        Ok(image)
    }

    pub fn from_image_data(image_data: Arc<ImageData>, hidpi_scale: f32) -> Image {
        Image {
            data: Cell::new(DataStage::Finished(image_data)),
            hidpi_scale,
            ..Default::default()
        }
    }

    pub fn set_link(&mut self, link: String) {
        self.is_link = Some(link);
    }

    pub fn with_align(mut self, align: Align) -> Self {
        self.is_aligned = Some(align);
        self
    }

    pub fn with_size(mut self, size: ImageSize) -> Self {
        self.size = Some(size);
        self
    }

    pub fn dimensions_from_image_size(&mut self, size: &ImageSize) -> Option<(u32, u32)> {
        let image_dimensions = self.buffer_dimensions()?;
        match size {
            ImageSize::PxWidth(px_width) => Some((
                *px_width,
                ((*px_width as f32 / image_dimensions.0 as f32) * image_dimensions.1 as f32) as u32,
            )),
            ImageSize::PxHeight(px_height) => Some((
                ((*px_height as f32 / image_dimensions.1 as f32) * image_dimensions.0 as f32)
                    as u32,
                *px_height,
            )),
        }
    }

    fn buffer_dimensions(&mut self) -> Option<(u32, u32)> {
        Some(self.try_get_data()?.ok()?.dimensions)
    }

    fn dimensions(&mut self, screen_size: Size, zoom: f32) -> Option<(u32, u32)> {
        let buffer_size = self.buffer_dimensions()?;
        let mut buffer_size = (buffer_size.0 as f32 * zoom, buffer_size.1 as f32 * zoom);
        if let Some(Ok(image)) = self.try_get_data() {
            if image.scale {
                buffer_size.0 *= self.hidpi_scale;
                buffer_size.1 *= self.hidpi_scale;
            }
        }
        let max_width = screen_size.0 - 2. * DEFAULT_MARGIN;
        let dimensions = if let Some(size) = self.size.clone() {
            let dimensions = self.dimensions_from_image_size(&size)?;
            let target_dimensions = (
                (dimensions.0 as f32 * self.hidpi_scale * zoom) as u32,
                (dimensions.1 as f32 * self.hidpi_scale * zoom) as u32,
            );
            if target_dimensions.0 > max_width as u32 {
                (
                    max_width as u32,
                    ((max_width / buffer_size.0) * buffer_size.1) as u32,
                )
            } else {
                target_dimensions
            }
        } else if buffer_size.0 > max_width {
            (
                max_width as u32,
                ((max_width / buffer_size.0) * buffer_size.1) as u32,
            )
        } else {
            (buffer_size.0 as u32, buffer_size.1 as u32)
        };
        Some(dimensions)
    }

    pub fn size(&mut self, screen_size: Size, zoom: f32) -> Option<Size> {
        self.dimensions(screen_size, zoom)
            .map(|d| (d.0 as f32, d.1 as f32))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct ImageVertex {
    pos: [f32; 3],
    tex_coords: [f32; 2],
}

pub struct ImageRenderer {
    pub render_pipeline: wgpu::RenderPipeline,
    pub index_buf: wgpu::Buffer,
    pub bindgroup_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}

pub fn point(x: f32, y: f32, position: Point, size: Size, screen: Size) -> [f32; 3] {
    let scale_x = size.0 / screen.0;
    let scale_y = size.1 / screen.1;
    let shift_x = (position.0 / screen.0) * 2.;
    let shift_y = (position.1 / screen.1) * 2.;
    let new_x = (x * scale_x) - (1. - scale_x) + shift_x;
    let new_y = (y * scale_y) + (1. - scale_y) - shift_y;
    [new_x, new_y, 0.]
}

impl ImageRenderer {
    pub fn new(device: &Device, format: &TextureFormat) -> Self {
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ImageVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        }];

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/image.wgsl"))),
        });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: *format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        const INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            render_pipeline: image_pipeline,
            index_buf,
            bindgroup_layout: texture_bind_group_layout,
            sampler,
        }
    }

    pub fn vertex_buf(device: &Device, pos: Point, size: Size, screen_size: Size) -> wgpu::Buffer {
        let vertices: &[ImageVertex] = &[
            // TOP LEFT
            ImageVertex {
                pos: point(-1.0, 1.0, pos, size, screen_size),
                tex_coords: [0.0, 0.0],
            },
            // BOTTOM LEFT
            ImageVertex {
                pos: point(-1.0, -1.0, pos, size, screen_size),
                tex_coords: [0.0, 1.0],
            },
            // BOTTOM RIGHT
            ImageVertex {
                pos: point(1.0, -1.0, pos, size, screen_size),
                tex_coords: [1.0, 1.0],
            },
            // TOP RIGHT
            ImageVertex {
                pos: point(1.0, 1.0, pos, size, screen_size),
                tex_coords: [1.0, 0.0],
            },
        ];
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }
}
