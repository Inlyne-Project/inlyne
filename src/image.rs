use crate::positioner::DEFAULT_MARGIN;
use crate::utils::{usize_in_mib, Align, Point, Size};
use async_once_cell::unpin::Lazy;
use bytemuck::{Pod, Zeroable};
use image::{ImageBuffer, RgbaImage};
use lz4_flex::frame::{BlockSize, FrameDecoder, FrameEncoder, FrameInfo};
use resvg::usvg_text_layout::TreeTextToPath;
use std::io::{self, Cursor, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;
use wgpu::{Device, TextureFormat};

use std::borrow::Cow;

#[derive(Debug, Clone)]
pub enum ImageSize {
    PxWidth(u32),
    PxHeight(u32),
}

#[derive(Debug, Default, Clone)]
pub struct ImageData {
    lz4_blob: Vec<u8>,
    scale: bool,
    dimensions: (u32, u32),
}

impl ImageData {
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

pub struct Image {
    pub image: Arc<Lazy<anyhow::Result<ImageData>>>,
    pub is_aligned: Option<Align>,
    pub size: Option<ImageSize>,
    pub bind_group: Option<Arc<wgpu::BindGroup>>,
    pub is_link: Option<String>,
    pub hidpi_scale: f32,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            image: Arc::new(Lazy::new(Box::pin(async { Ok(ImageData::default()) }))),
            is_aligned: Default::default(),
            size: Default::default(),
            bind_group: Default::default(),
            is_link: Default::default(),
            hidpi_scale: Default::default(),
        }
    }
}

impl Image {
    pub async fn create_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        bindgroup_layout: &wgpu::BindGroupLayout,
    ) {
        let dimensions = self.buffer_dimensions().unwrap();
        if let Ok(image) = self.image.get().await {
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
            self.bind_group = Some(Arc::new(bind_group));
        }
    }

    pub fn from_src(src: String, file_path: PathBuf, hidpi_scale: f32) -> anyhow::Result<Image> {
        let image = async move {
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

            if let Ok(image) = image::load_from_memory(&image_data) {
                Ok(ImageData::new(image.into_rgba8(), true))
            } else {
                let opt = usvg::Options::default();
                let mut rtree = usvg::Tree::from_data(&image_data, &opt)?;
                let mut fontdb = resvg::usvg_text_layout::fontdb::Database::new();
                fontdb.load_system_fonts();
                rtree.convert_text(&fontdb);
                let pixmap_size = rtree.size.to_screen_size();
                let mut pixmap = tiny_skia::Pixmap::new(
                    (pixmap_size.width() as f32 * hidpi_scale) as u32,
                    (pixmap_size.height() as f32 * hidpi_scale) as u32,
                )
                .unwrap();
                resvg::render(
                    &rtree,
                    usvg::FitTo::Zoom(hidpi_scale),
                    tiny_skia::Transform::default(),
                    pixmap.as_mut(),
                )
                .unwrap();
                Ok(ImageData::new(
                    ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.data().into())
                        .unwrap(),
                    false,
                ))
            }
        };

        Ok(Image {
            image: Arc::new(Lazy::new(Box::pin(image))),
            hidpi_scale,
            ..Default::default()
        })
    }

    pub fn from_image_data(
        image_data: Arc<Lazy<anyhow::Result<ImageData>>>,
        hidpi_scale: f32,
    ) -> Image {
        Image {
            image: image_data,
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

    pub fn dimensions_from_image_size(&self, size: &ImageSize) -> Option<(u32, u32)> {
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

    fn buffer_dimensions(&self) -> Option<(u32, u32)> {
        Some(self.image.try_get()?.as_ref().ok()?.dimensions)
    }

    fn dimensions(&self, screen_size: Size, zoom: f32) -> Option<(u32, u32)> {
        let buffer_size = self.buffer_dimensions()?;
        let mut buffer_size = (buffer_size.0 as f32 * zoom, buffer_size.1 as f32 * zoom);
        if let Some(Ok(image)) = self.image.try_get() {
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
