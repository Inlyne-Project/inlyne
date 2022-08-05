use crate::image::ImageRenderer;
use crate::{Element, InlyneEvent};
use bytemuck::{Pod, Zeroable};
use lyon::geom::euclid::Point2D;
use lyon::geom::Box2D;
use lyon::tessellation::*;
use std::borrow::Cow;
use std::ops::{Deref, Range};
use wgpu::util::DeviceExt;
use wgpu::{util::StagingBelt, TextureFormat};
use wgpu::{BindGroup, Buffer};
use wgpu_glyph::{ab_glyph, GlyphBrush, GlyphBrushBuilder};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

pub const DEFAULT_PADDING: f32 = 5.;
pub const DEFAULT_MARGIN: f32 = 100.;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

pub struct Rect {
    pub pos: (f32, f32),
    pub size: (f32, f32),
}

impl Rect {
    pub fn new(pos: (f32, f32), size: (f32, f32)) -> Rect {
        Rect { pos, size }
    }

    pub fn pos(&self) -> (f32, f32) {
        self.pos
    }

    pub fn size(&self) -> (f32, f32) {
        self.size
    }

    pub fn contains(&self, loc: (f32, f32)) -> bool {
        loc.0 >= self.pos.0
            && loc.0 <= self.pos.0 + self.size.0
            && loc.1 >= self.pos.1
            && loc.1 <= self.pos.1 + self.size.1
    }

    pub fn from_min_max(min: (f32, f32), max: (f32, f32)) -> Rect {
        Rect {
            pos: min,
            size: (max.0 - min.0, max.1 - min.1),
        }
    }
}

pub struct Positioned<T> {
    inner: T,
    pub bounds: Option<Rect>,
}

impl<T> Deref for Positioned<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> Positioned<T> {
    pub fn contains(&self, loc: (f32, f32)) -> bool {
        self.bounds.as_ref().unwrap().contains(loc)
    }
}

impl<T> Positioned<T> {
    pub fn new(item: T) -> Positioned<T> {
        Positioned {
            inner: item,
            bounds: None,
        }
    }
}

pub struct Renderer {
    pub config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub render_pipeline: wgpu::RenderPipeline,
    pub queue: wgpu::Queue,
    pub glyph_brush: GlyphBrush<()>,
    pub staging_belt: StagingBelt,
    pub elements: Vec<Positioned<Element>>,
    pub scroll_y: f32,
    pub lyon_buffer: VertexBuffers<Vertex, u16>,
    pub reserved_height: f32,
    pub hidpi_scale: f32,
    pub image_renderer: ImageRenderer,
    pub eventloop_proxy: EventLoopProxy<InlyneEvent>,
    pub occluded: bool,
}

impl Renderer {
    pub const fn screen_width(&self) -> f32 {
        self.config.width as f32
    }

    pub const fn screen_height(&self) -> f32 {
        self.config.height as f32
    }

    pub const fn screen_size(&self) -> (f32, f32) {
        (self.config.width as f32, self.config.height as f32)
    }

    pub async fn new(window: &Window, eventloop_proxy: EventLoopProxy<InlyneEvent>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                // Request an adapter which can render to our surface
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        // Create the logical device and command queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                    limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        // Create staging belt
        let staging_belt = wgpu::util::StagingBelt::new(1024);

        // Load the shaders from disk
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/shader.wgsl"))),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let supported_formats = surface.get_supported_formats(&adapter);
        let swapchain_format = if supported_formats.contains(&TextureFormat::Rgba16Float) {
            TextureFormat::Rgba16Float
        } else {
            supported_formats[0]
        };

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        }];

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                targets: &[Some(swapchain_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        surface.configure(&device, &config);
        let image_renderer = ImageRenderer::new(&device, &swapchain_format);

        let roboto_reg =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/RobotoMono-Regular.ttf"))
                .unwrap();
        let roboto_bold =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/RobotoMono-Bold.ttf"))
                .unwrap();
        let sf_reg =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/SFUIText-Regular.otf"))
                .unwrap();
        let sf_bold =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/SFUIText-Bold.otf")).unwrap();

        let mut glyph_brush =
            GlyphBrushBuilder::using_font(sf_reg).build(&device, swapchain_format);
        glyph_brush.add_font(sf_bold);
        glyph_brush.add_font(roboto_reg);
        glyph_brush.add_font(roboto_bold);
        let lyon_buffer: VertexBuffers<Vertex, u16> = VertexBuffers::new();

        Self {
            config,
            surface,
            device,
            render_pipeline,
            queue,
            glyph_brush,
            staging_belt,
            elements: Vec::new(),
            scroll_y: 0.,
            lyon_buffer,
            reserved_height: DEFAULT_PADDING,
            hidpi_scale: window.scale_factor() as f32,
            image_renderer,
            eventloop_proxy,
            occluded: false,
        }
    }

    pub fn draw_scrollbar(&mut self, reserved_height: f32) -> u32 {
        let screen_height = self.screen_height();
        let screen_width = self.config.width as f32;
        let top = if screen_height < reserved_height {
            1. - (self.scroll_y / (reserved_height - screen_height)
                * (1. - (screen_height / reserved_height)))
                * 2.
        } else {
            1.
        };
        let bottom = if screen_height < reserved_height {
            top - ((screen_height / reserved_height) * 2.)
        } else {
            top - 2.
        };
        let mut fill_tessellator = FillTessellator::new();

        {
            // Compute the tessellation.
            fill_tessellator
                .tessellate_rectangle(
                    &Box2D::new(
                        Point2D::from((1. - 50. / screen_width, top)),
                        Point2D::from((1.0, bottom)),
                    ),
                    &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut self.lyon_buffer, |vertex: FillVertex| Vertex {
                        pos: [vertex.position().x, vertex.position().y, 0.0],
                        color: [0.3, 0.3, 0.3, 1.0],
                    }),
                )
                .unwrap();
        }

        self.lyon_buffer.indices.len() as u32
    }

    pub fn render_elements(&mut self, reserved_height: f32) -> Vec<Range<u32>> {
        let mut indice_ranges = Vec::new();
        let mut _prev_indice_num = 0;
        let screen_size = self.screen_size();
        for element in &mut self.elements {
            let Rect { pos, size } = element.bounds.as_ref().expect("Element not positioned");
            let scrolled_pos = (pos.0, pos.1 - self.scroll_y);
            // Dont render off screen elements
            if scrolled_pos.1 + size.1 <= 0. || scrolled_pos.1 >= screen_size.1 {
                continue;
            }
            match &mut element.inner {
                Element::TextBox(text_box) => {
                    self.glyph_brush.queue(text_box.glyph_section(
                        (pos.0, pos.1),
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, screen_size.1),
                        self.hidpi_scale,
                    ));
                }
                Element::Image(_) => {}
                Element::Spacer(_) => {}
            }
        }

        indice_ranges.push(_prev_indice_num..self.draw_scrollbar(reserved_height));
        indice_ranges
    }

    pub fn image_bindgroups(&mut self) -> Vec<(BindGroup, Buffer)> {
        let screen_size = self.screen_size();
        let mut bind_groups = Vec::new();
        for element in &mut self.elements {
            let Rect { pos, size } = element.bounds.as_ref().unwrap();
            let pos = (pos.0, pos.1 - self.scroll_y);
            if let Element::Image(ref mut image) = element.inner {
                let dimensions = image.buffer_dimensions();
                if let Some(image_data) = image.get_image_data().as_deref() {
                    let texture_size = wgpu::Extent3d {
                        width: dimensions.0,
                        height: dimensions.1,
                        depth_or_array_layers: 1,
                    };
                    let diffuse_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        // All textures are stored as 3D, we represent our 2D texture
                        // by setting depth to 1.
                        size: texture_size,
                        mip_level_count: 1, // We'll talk about this a little later
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        // Most images are stored using sRGB so we need to reflect that here.
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        // TEXTURE_BINDING tells wgpu that we want to use this texture in shaders
                        // COPY_DST means that we want to copy data to this texture
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        label: Some("diffuse_texture"),
                    });
                    self.queue.write_texture(
                        // Tells wgpu where to copy the pixel data
                        wgpu::ImageCopyTexture {
                            texture: &diffuse_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        // The actual pixel data
                        image_data,
                        // The layout of the texture
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: std::num::NonZeroU32::new(4 * dimensions.0),
                            rows_per_image: std::num::NonZeroU32::new(dimensions.1),
                        },
                        texture_size,
                    );

                    let diffuse_texture_view =
                        diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let diffuse_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                        address_mode_u: wgpu::AddressMode::ClampToEdge,
                        address_mode_v: wgpu::AddressMode::ClampToEdge,
                        address_mode_w: wgpu::AddressMode::ClampToEdge,
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Nearest,
                        mipmap_filter: wgpu::FilterMode::Nearest,
                        ..Default::default()
                    });

                    let diffuse_bind_group =
                        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            layout: &self.image_renderer.bindgroup_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(
                                        &diffuse_texture_view,
                                    ),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                                },
                            ],
                            label: Some("diffuse_bind_group"),
                        });

                    let vertex_buf =
                        ImageRenderer::vertex_buf(&self.device, pos, *size, screen_size);
                    bind_groups.push((diffuse_bind_group, vertex_buf));
                }
            }
        }
        bind_groups
    }
    pub fn redraw(&mut self, reserved_height: f32) {
        if self.occluded {
            return;
        }
        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.lyon_buffer.indices.clear();
        self.lyon_buffer.vertices.clear();
        let indice_ranges = self.render_elements(reserved_height);
        let vertex_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&self.lyon_buffer.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&self.lyon_buffer.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let image_bindgroups = self.image_bindgroups();

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
            rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            for range in indice_ranges {
                rpass.draw_indexed(range, 0, 0..1);
            }

            rpass.set_pipeline(&self.image_renderer.render_pipeline);
            for (bindgroup, vertex_buf) in image_bindgroups.iter() {
                rpass.set_bind_group(0, bindgroup, &[]);
                rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                rpass.draw_indexed(0..6, 0, 0..1);
            }
        }

        let screen_size = self.screen_size();
        // Draw the text!
        self.glyph_brush
            .draw_queued_with_transform(
                &self.device,
                &mut self.staging_belt,
                &mut encoder,
                &view,
                [
                    2.0 / screen_size.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    -2.0 / screen_size.1,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    -1.0,
                    1.0 + (self.scroll_y * 2. / (screen_size.1)),
                    0.0,
                    1.0,
                ],
            )
            .expect("Draw queued");

        // Submit the work!
        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();

        // Recall unused staging buffers
        self.staging_belt.recall();
    }

    pub fn position(&mut self, element_id: usize) {
        let screen_size = self.screen_size();
        let element = &mut self.elements[element_id];
        match &element.inner {
            Element::TextBox(text_box) => {
                let indent = text_box.indent;
                let pos = (DEFAULT_MARGIN + indent, self.reserved_height);

                let size = text_box.size(
                    &mut self.glyph_brush,
                    pos,
                    (screen_size.0 - pos.0 - DEFAULT_MARGIN, screen_size.1),
                    self.hidpi_scale,
                );

                element.bounds = Some(Rect { pos, size });
                self.reserved_height += size.1;
            }
            Element::Spacer(spacer) => {
                element.bounds = Some(Rect {
                    pos: (0., self.reserved_height),
                    size: (0., spacer.space),
                });
                self.reserved_height += spacer.space;
            }
            Element::Image(image) => {
                let size = image.size(self.hidpi_scale, screen_size);
                element.bounds = Some(Rect {
                    pos: (DEFAULT_MARGIN, self.reserved_height),
                    size,
                });
                match image.is_aligned {
                    Some(Align::Center) => {
                        element.bounds = Some(Rect {
                            pos: (screen_size.0 / 2. - size.0 / 2., self.reserved_height),
                            size,
                        });
                    }
                    _ => {}
                }
                self.reserved_height += size.1;
            }
        }
        self.reserved_height += DEFAULT_PADDING;
    }
    pub fn reposition(&mut self) {
        self.reserved_height = DEFAULT_PADDING;

        for element_id in 0..self.elements.len() {
            self.position(element_id);
        }
    }

    pub fn push(&mut self, mut element: Element) {
        if let Element::Image(ref mut image) = element {
            image.add_callback(self.eventloop_proxy.clone());
        }
        let last_element_id = self.elements.len();
        self.elements.push(Positioned::new(element));
        self.position(last_element_id)
    }
}

#[derive(Debug)]
pub struct Spacer {
    space: f32,
}

impl Spacer {
    pub fn new(space: f32) -> Spacer {
        Spacer { space }
    }
}

#[derive(Debug, Clone)]
pub enum Align {
    Left,
    Center,
    Right,
    Justify,
}
