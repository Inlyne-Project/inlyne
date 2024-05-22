mod decode;
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{
    fs,
    io::{self, Read},
};

use crate::debug_impls::{DebugBytesPrefix, DebugInline};
use crate::interpreter::ImageCallback;
use crate::metrics::{histogram, HistTag};
use crate::positioner::DEFAULT_MARGIN;
use crate::utils::{usize_in_mib, Align, Point, Size};

use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use image::{ImageBuffer, RgbaImage};
use resvg::{tiny_skia, usvg};
use smart_debug::SmartDebug;
use wgpu::util::DeviceExt;
use wgpu::{BindGroup, Device, TextureFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Px(u32);

impl FromStr for Px {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let px: u32 = s.strip_suffix("px").unwrap_or(s).parse()?;
        Ok(Self(px))
    }
}

impl From<u32> for Px {
    fn from(px: u32) -> Self {
        Self(px)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ImageSize {
    PxWidth(Px),
    PxHeight(Px),
}

impl ImageSize {
    pub fn width<P: Into<Px>>(px: P) -> Self {
        Self::PxWidth(px.into())
    }

    pub fn height<P: Into<Px>>(px: P) -> Self {
        Self::PxHeight(px.into())
    }
}

#[derive(SmartDebug, Default, Clone)]
pub struct ImageData {
    #[debug(wrapper = DebugBytesPrefix)]
    lz4_blob: Vec<u8>,
    scale: bool,
    #[debug(wrapper = DebugInline)]
    dimensions: (u32, u32),
}

impl ImageData {
    fn load(bytes: &[u8], scale: bool) -> anyhow::Result<Self> {
        let (lz4_blob, dimensions) = decode::decode_and_compress(bytes)?;
        Ok(Self {
            lz4_blob,
            scale,
            dimensions,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        decode::lz4_decompress(&self.lz4_blob, self.rgba_image_byte_size())
            .expect("Size matches and I/O is in memory")
    }

    fn new(image: RgbaImage, scale: bool) -> Self {
        let dimensions = image.dimensions();

        let start = Instant::now();
        let lz4_blob =
            decode::lz4_compress(&mut io::Cursor::new(image.as_raw())).expect("I/O is in memory");
        tracing::debug!(
            "Compressing SVG image:\n- Full {:.2} MiB\n- Compressed {:.2} MiB\n- Time {:.2?}",
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

#[derive(SmartDebug, Default)]
pub struct Image {
    #[debug(skip_fn = debug_ignore_image_data)]
    pub image_data: Arc<Mutex<Option<ImageData>>>,
    #[debug(skip_fn = Option::is_none, wrapper = DebugInline)]
    pub is_aligned: Option<Align>,
    #[debug(skip_fn = Option::is_none, wrapper = DebugInline)]
    pub size: Option<ImageSize>,
    #[debug(skip)]
    pub bind_group: Option<Arc<wgpu::BindGroup>>,
    #[debug(skip_fn = Option::is_none, wrapper = DebugInline)]
    pub is_link: Option<String>,
    #[debug(skip)]
    pub hidpi_scale: f32,
}

fn debug_ignore_image_data(mutex: &Mutex<Option<ImageData>>) -> bool {
    match mutex.lock() {
        Ok(data) => data.is_none(),
        Err(_) => true,
    }
}

impl Image {
    pub fn create_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampler: &wgpu::Sampler,
        bindgroup_layout: &wgpu::BindGroupLayout,
    ) -> Option<Arc<BindGroup>> {
        let dimensions = self.buffer_dimensions()?;
        if dimensions.0 == 0 || dimensions.1 == 0 {
            tracing::warn!("Invalid buffer dimensions");
            return None;
        }

        let start = Instant::now();
        let rgba_image = self
            .image_data
            .lock()
            .unwrap()
            .as_ref()
            .map(|image| image.to_bytes())?;

        tracing::debug!("Decompressing image: Time {:.2?}", start.elapsed());

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
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
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
        Some(bind_group)
    }

    pub fn from_src(
        src: String,
        file_path: PathBuf,
        hidpi_scale: f32,
        image_callback: Box<dyn ImageCallback + Send>,
    ) -> anyhow::Result<Image> {
        let image_data = Arc::new(Mutex::new(None));
        let image_data_clone = image_data.clone();

        std::thread::spawn(move || {
            let start = Instant::now();

            let mut src_path = PathBuf::from(&src);
            if src_path.is_relative() {
                if let Some(parent_dir) = file_path.parent() {
                    src_path = parent_dir.join(src_path.strip_prefix("./").unwrap_or(&src_path));
                }
            }

            let image_data = if let Ok(img_file) = fs::read(&src_path) {
                img_file
            } else if let Ok(bytes) = http_get_image(&src) {
                bytes
            } else {
                tracing::warn!("Request for image from {} failed", src_path.display());
                return;
            };

            let image = if let Ok(image) = ImageData::load(&image_data, true) {
                image
            } else {
                let opt = usvg::Options::default();
                let mut fontdb = usvg::fontdb::Database::new();
                fontdb.load_system_fonts();
                // TODO: yes all of this image loading is very messy and could use a refactor
                let Ok(mut tree) = usvg::Tree::from_data(&image_data, &opt) else {
                    tracing::warn!(
                        "Failed loading image:\n- src: {}\n- src_path: {}",
                        src,
                        src_path.display()
                    );
                    let image =
                        ImageData::load(include_bytes!("../../assets/img/broken.png"), false)
                            .unwrap();
                    *image_data_clone.lock().unwrap() = Some(image);
                    image_callback.loaded_image(src, image_data_clone);
                    return;
                };
                tree.size = tree.size.scale_to(
                    tiny_skia::Size::from_wh(
                        tree.size.width() * hidpi_scale,
                        tree.size.height() * hidpi_scale,
                    )
                    .unwrap(),
                );
                tree.postprocess(Default::default(), &fontdb);
                let mut pixmap =
                    tiny_skia::Pixmap::new(tree.size.width() as u32, tree.size.height() as u32)
                        .context("Couldn't create svg pixmap")
                        .unwrap();
                resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
                ImageData::new(
                    ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.data().into())
                        .context("Svg buffer has invalid dimensions")
                        .unwrap(),
                    false,
                )
            };

            *image_data_clone.lock().unwrap() = Some(image);
            histogram!(HistTag::ImageLoad).record(start.elapsed());
            image_callback.loaded_image(src, image_data_clone);
        });

        let image = Image {
            image_data,
            hidpi_scale,
            ..Default::default()
        };

        Ok(image)
    }

    pub fn from_image_data(image_data: Arc<Mutex<Option<ImageData>>>, hidpi_scale: f32) -> Image {
        Image {
            image_data,
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
                px_width.0,
                ((px_width.0 as f32 / image_dimensions.0 as f32) * image_dimensions.1 as f32)
                    as u32,
            )),
            ImageSize::PxHeight(px_height) => Some((
                ((px_height.0 as f32 / image_dimensions.1 as f32) * image_dimensions.0 as f32)
                    as u32,
                px_height.0,
            )),
        }
    }

    fn buffer_dimensions(&self) -> Option<(u32, u32)> {
        Some(self.image_data.lock().unwrap().as_ref()?.dimensions)
    }

    fn dimensions(&mut self, screen_size: Size, zoom: f32) -> Option<(u32, u32)> {
        let buffer_size = self.buffer_dimensions()?;
        let mut buffer_size = (buffer_size.0 as f32 * zoom, buffer_size.1 as f32 * zoom);
        if let Some(image) = self.image_data.lock().as_deref().unwrap() {
            if image.scale {
                buffer_size.0 *= self.hidpi_scale;
                buffer_size.1 *= self.hidpi_scale;
            }
        }
        let max_width = screen_size.0 - 2. * DEFAULT_MARGIN;
        let dimensions = if let Some(size) = self.size {
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

pub fn http_get_image(url: &str) -> anyhow::Result<Vec<u8>> {
    const USER_AGENT: &str = concat!(
        "inlyne ",
        env!("CARGO_PKG_VERSION"),
        " https://github.com/Inlyne-Project/inlyne"
    );

    const LIMIT: usize = 20 * 1_024 * 1_024;

    let resp = ureq::get(url).set("User-Agent", USER_AGENT).call()?;
    let len = resp
        .header("Content-Length")
        .and_then(|len| len.parse::<usize>().ok());
    let mut body = Vec::with_capacity(len.unwrap_or(0).clamp(0, LIMIT));
    resp.into_reader()
        .take(u64::try_from(LIMIT).unwrap())
        .read_to_end(&mut body)?;
    Ok(body)
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
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/image.wgsl"))),
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
