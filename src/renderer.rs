use crate::color::Theme;
use crate::image::ImageRenderer;
use crate::table::{TABLE_COL_GAP, TABLE_ROW_GAP};
use crate::utils::{Align, Rect};
use crate::{Element, InlyneEvent};
use bytemuck::{Pod, Zeroable};
use lyon::geom::euclid::Point2D;
use lyon::geom::Box2D;
use lyon::tessellation::*;
use std::borrow::Cow;
use std::ops::{Deref, Range};
use std::sync::Arc;
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
    pub theme: Theme,
    pub selection: Option<((f32, f32), (f32, f32))>,
    pub selection_text: String,
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

    pub async fn new(
        window: &Window,
        eventloop_proxy: EventLoopProxy<InlyneEvent>,
        theme: Theme,
        hidpi_scale: f32,
    ) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let staging_belt = wgpu::util::StagingBelt::new(1024);

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
        let roboto_reg_italic =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/RobotoMono-Italic.ttf"))
                .unwrap();
        let roboto_bold =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/RobotoMono-Bold.ttf"))
                .unwrap();
        let roboto_bold_italic =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/RobotoMono-BoldItalic.ttf"))
                .unwrap();
        let sf_reg =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/SFUIText-Regular.otf"))
                .unwrap();
        let sf_reg_italic =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/SFUIText-Italic.otf"))
                .unwrap();
        let sf_bold =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/SFUIText-Bold.otf")).unwrap();
        let sf_bold_italic =
            ab_glyph::FontArc::try_from_slice(include_bytes!("./fonts/SFUIText-BoldItalicG1.otf"))
                .unwrap();

        let mut glyph_brush =
            GlyphBrushBuilder::using_font(sf_reg).build(&device, swapchain_format);
        glyph_brush.add_font(sf_reg_italic);
        glyph_brush.add_font(sf_bold);
        glyph_brush.add_font(sf_bold_italic);
        glyph_brush.add_font(roboto_reg);
        glyph_brush.add_font(roboto_reg_italic);
        glyph_brush.add_font(roboto_bold);
        glyph_brush.add_font(roboto_bold_italic);
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
            hidpi_scale,
            image_renderer,
            eventloop_proxy,
            theme,
            selection: None,
            selection_text: String::new(),
        }
    }

    fn draw_scrollbar(&mut self, reserved_height: f32) -> u32 {
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

    fn render_elements(&mut self, reserved_height: f32) -> Vec<Range<u32>> {
        let mut indice_ranges = Vec::new();
        let mut _prev_indice_num = 0;
        let screen_size = self.screen_size();
        for element in &mut self.elements {
            let Rect { pos, size, max: _ } =
                element.bounds.as_ref().expect("Element not positioned");
            let scrolled_pos = (pos.0, pos.1 - self.scroll_y);
            // Dont render off screen elements
            // FIX ME
            if self.selection.is_none() {
                if scrolled_pos.1 + size.1 <= 0. {
                    continue;
                } else if scrolled_pos.1 >= screen_size.1 {
                    break;
                }
            }

            match &mut element.inner {
                Element::TextBox(text_box) => {
                    let bounds = (screen_size.0 - pos.0 - DEFAULT_MARGIN, screen_size.1);
                    self.glyph_brush
                        .queue(&text_box.glyph_section(*pos, bounds));
                    let mut fill_tessellator = FillTessellator::new();
                    if text_box.is_code_block || text_box.is_quote_block.is_some() {
                        let color = if text_box.is_code_block {
                            self.theme.code_block_color
                        } else {
                            self.theme.quote_block_color
                        };

                        let mut min = (scrolled_pos.0 - 10., scrolled_pos.1);
                        let max = (min.0 + bounds.0 + 10., min.1 + size.1 + 5.);
                        if let Some(nest) = text_box.is_quote_block {
                            min.0 -= (nest - 1) as f32 * DEFAULT_MARGIN / 2.;
                        }
                        fill_tessellator
                            .tessellate_rectangle(
                                &Box2D::new(
                                    Point2D::from(point(min.0, min.1, screen_size)),
                                    Point2D::from(point(max.0, max.1, screen_size)),
                                ),
                                &FillOptions::default(),
                                &mut BuffersBuilder::new(
                                    &mut self.lyon_buffer,
                                    |vertex: FillVertex| Vertex {
                                        pos: [vertex.position().x, vertex.position().y, 0.0],
                                        color,
                                    },
                                ),
                            )
                            .unwrap();
                        indice_ranges.push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                        _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                    }
                    if let Some(nest) = text_box.is_quote_block {
                        for n in 0..nest {
                            let nest_indent = n as f32 * DEFAULT_MARGIN / 2.;
                            let min = (scrolled_pos.0 - 20. - nest_indent, scrolled_pos.1);
                            let max = (scrolled_pos.0 - 10. - nest_indent, min.1 + size.1 + 5.);
                            fill_tessellator
                                .tessellate_rectangle(
                                    &Box2D::new(
                                        Point2D::from(point(min.0, min.1, screen_size)),
                                        Point2D::from(point(max.0, max.1, screen_size)),
                                    ),
                                    &FillOptions::default(),
                                    &mut BuffersBuilder::new(
                                        &mut self.lyon_buffer,
                                        |vertex: FillVertex| Vertex {
                                            pos: [vertex.position().x, vertex.position().y, 0.0],
                                            color: self.theme.select_color,
                                        },
                                    ),
                                )
                                .unwrap();
                            indice_ranges
                                .push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                            _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                        }
                    }
                    if let Some(ref lines) =
                        text_box.render_lines(&mut self.glyph_brush, scrolled_pos, bounds)
                    {
                        for line in lines {
                            let min = line.0;
                            let max = (line.1 .0, line.1 .1 + 2.);
                            fill_tessellator
                                .tessellate_rectangle(
                                    &Box2D::new(
                                        Point2D::from(point(min.0, min.1, screen_size)),
                                        Point2D::from(point(max.0, max.1, screen_size)),
                                    ),
                                    &FillOptions::default(),
                                    &mut BuffersBuilder::new(
                                        &mut self.lyon_buffer,
                                        |vertex: FillVertex| Vertex {
                                            pos: [vertex.position().x, vertex.position().y, 0.0],
                                            color: self.theme.text_color,
                                        },
                                    ),
                                )
                                .unwrap();
                            indice_ranges
                                .push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                            _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                        }
                    }
                    if let Some(selection) = self.selection {
                        let (selection_rects, selection_text) = text_box.render_selection(
                            &mut self.glyph_brush,
                            *pos,
                            bounds,
                            selection,
                        );
                        self.selection_text.push_str(&selection_text);
                        for rect in selection_rects {
                            fill_tessellator
                                .tessellate_rectangle(
                                    &Box2D::new(
                                        Point2D::from(point(
                                            rect.pos.0,
                                            rect.pos.1 - self.scroll_y,
                                            screen_size,
                                        )),
                                        Point2D::from(point(
                                            rect.max.0,
                                            rect.max.1 - self.scroll_y,
                                            screen_size,
                                        )),
                                    ),
                                    &FillOptions::default(),
                                    &mut BuffersBuilder::new(
                                        &mut self.lyon_buffer,
                                        |vertex: FillVertex| Vertex {
                                            pos: [vertex.position().x, vertex.position().y, 0.0],
                                            color: self.theme.select_color,
                                        },
                                    ),
                                )
                                .unwrap();
                            indice_ranges
                                .push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                            _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                        }
                    }
                }
                Element::Table(table) => {
                    let mut fill_tessellator = FillTessellator::new();
                    let row_heights = table.row_heights(
                        &mut self.glyph_brush,
                        *pos,
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                    );
                    let column_widths = table.column_widths(
                        &mut self.glyph_brush,
                        *pos,
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                    );
                    let mut x = 0.;
                    let mut y = 0.;

                    let header_height = row_heights.first().unwrap();
                    for (col, width) in column_widths.iter().enumerate() {
                        let text_box = table.headers.get(col).unwrap();
                        let bounds = (screen_size.0 - pos.0 - x - DEFAULT_MARGIN, f32::INFINITY);
                        self.glyph_brush
                            .queue(&text_box.glyph_section((pos.0 + x, pos.1 + y), bounds));
                        if let Some(selection) = self.selection {
                            let (selection_rects, selection_text) = text_box.render_selection(
                                &mut self.glyph_brush,
                                (pos.0 + x, pos.1 + y),
                                bounds,
                                selection,
                            );
                            self.selection_text.push_str(&selection_text);
                            for rect in selection_rects {
                                fill_tessellator
                                    .tessellate_rectangle(
                                        &Box2D::new(
                                            Point2D::from(point(
                                                rect.pos.0,
                                                rect.pos.1 - self.scroll_y,
                                                screen_size,
                                            )),
                                            Point2D::from(point(
                                                rect.max.0,
                                                rect.max.1 - self.scroll_y,
                                                screen_size,
                                            )),
                                        ),
                                        &FillOptions::default(),
                                        &mut BuffersBuilder::new(
                                            &mut self.lyon_buffer,
                                            |vertex: FillVertex| Vertex {
                                                pos: [
                                                    vertex.position().x,
                                                    vertex.position().y,
                                                    0.0,
                                                ],
                                                color: self.theme.select_color,
                                            },
                                        ),
                                    )
                                    .unwrap();
                                indice_ranges
                                    .push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                                _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                            }
                        }
                        x += width + TABLE_COL_GAP;
                    }
                    y += header_height + (TABLE_ROW_GAP / 2.);
                    {
                        let min = (scrolled_pos.0, scrolled_pos.1 + y);
                        let max = (scrolled_pos.0 + x, scrolled_pos.1 + y + 3.);
                        fill_tessellator
                            .tessellate_rectangle(
                                &Box2D::new(
                                    Point2D::from(point(min.0, min.1, screen_size)),
                                    Point2D::from(point(max.0, max.1, screen_size)),
                                ),
                                &FillOptions::default(),
                                &mut BuffersBuilder::new(
                                    &mut self.lyon_buffer,
                                    |vertex: FillVertex| Vertex {
                                        pos: [vertex.position().x, vertex.position().y, 0.0],
                                        color: self.theme.text_color,
                                    },
                                ),
                            )
                            .unwrap();
                        indice_ranges.push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                        _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                    }

                    y += TABLE_ROW_GAP / 2.;
                    for (row, height) in row_heights.iter().skip(1).enumerate() {
                        let mut x = 0.;
                        for (col, width) in column_widths.iter().enumerate() {
                            if let Some(row) = table.rows.get(row) {
                                if let Some(text_box) = row.get(col) {
                                    let bounds =
                                        (screen_size.0 - pos.0 - x - DEFAULT_MARGIN, f32::INFINITY);
                                    self.glyph_brush.queue(
                                        &text_box.glyph_section((pos.0 + x, pos.1 + y), bounds),
                                    );

                                    if let Some(selection) = self.selection {
                                        let (selection_rects, selection_text) = text_box
                                            .render_selection(
                                                &mut self.glyph_brush,
                                                (pos.0 + x, pos.1 + y),
                                                bounds,
                                                selection,
                                            );
                                        self.selection_text.push_str(&selection_text);
                                        for rect in selection_rects {
                                            fill_tessellator
                                                .tessellate_rectangle(
                                                    &Box2D::new(
                                                        Point2D::from(point(
                                                            rect.pos.0,
                                                            rect.pos.1 - self.scroll_y,
                                                            screen_size,
                                                        )),
                                                        Point2D::from(point(
                                                            rect.max.0,
                                                            rect.max.1 - self.scroll_y,
                                                            screen_size,
                                                        )),
                                                    ),
                                                    &FillOptions::default(),
                                                    &mut BuffersBuilder::new(
                                                        &mut self.lyon_buffer,
                                                        |vertex: FillVertex| Vertex {
                                                            pos: [
                                                                vertex.position().x,
                                                                vertex.position().y,
                                                                0.0,
                                                            ],
                                                            color: self.theme.select_color,
                                                        },
                                                    ),
                                                )
                                                .unwrap();
                                            indice_ranges.push(
                                                _prev_indice_num
                                                    ..self.lyon_buffer.indices.len() as u32,
                                            );
                                            _prev_indice_num =
                                                self.lyon_buffer.indices.len() as u32;
                                        }
                                    }
                                }
                            }
                            x += width + TABLE_COL_GAP;
                        }
                        y += height + (TABLE_COL_GAP / 2.);
                        {
                            let min = (scrolled_pos.0, scrolled_pos.1 + y);
                            let max = (scrolled_pos.0 + x, scrolled_pos.1 + y + 3.);
                            fill_tessellator
                                .tessellate_rectangle(
                                    &Box2D::new(
                                        Point2D::from(point(min.0, min.1, screen_size)),
                                        Point2D::from(point(max.0, max.1, screen_size)),
                                    ),
                                    &FillOptions::default(),
                                    &mut BuffersBuilder::new(
                                        &mut self.lyon_buffer,
                                        |vertex: FillVertex| Vertex {
                                            pos: [vertex.position().x, vertex.position().y, 0.0],
                                            color: self.theme.code_block_color,
                                        },
                                    ),
                                )
                                .unwrap();
                            indice_ranges
                                .push(_prev_indice_num..self.lyon_buffer.indices.len() as u32);
                            _prev_indice_num = self.lyon_buffer.indices.len() as u32;
                        }
                        y += TABLE_ROW_GAP / 2.;
                    }
                }
                Element::Image(_) => {}
                Element::Spacer(_) => {}
            }
        }

        indice_ranges.push(_prev_indice_num..self.draw_scrollbar(reserved_height));
        indice_ranges
    }

    fn image_bindgroups(&mut self) -> Vec<(Arc<BindGroup>, Buffer)> {
        let screen_size = self.screen_size();
        let mut bind_groups = Vec::new();
        for element in &mut self.elements {
            let Rect { pos, size, max } = element.bounds.as_ref().unwrap();
            let pos = (pos.0, pos.1 - self.scroll_y);
            if max.1 <= 0. {
                continue;
            } else if pos.1 >= screen_size.1 {
                break;
            }
            if let Element::Image(ref mut image) = element.inner {
                if image.bind_group.is_none() {
                    image.create_bind_group(
                        &self.device,
                        &self.queue,
                        &self.image_renderer.sampler,
                        &self.image_renderer.bindgroup_layout,
                    );
                }
                if let Some(ref bind_group) = image.bind_group {
                    let vertex_buf =
                        ImageRenderer::vertex_buf(&self.device, pos, *size, screen_size);
                    bind_groups.push((bind_group.clone(), vertex_buf));
                }
            }
        }
        bind_groups
    }

    pub fn redraw(&mut self) {
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

        // Prepare and render elements that use lyon
        self.lyon_buffer.indices.clear();
        self.lyon_buffer.vertices.clear();
        self.selection_text = String::new();
        let indice_ranges = self.render_elements(self.reserved_height);
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

        // Prepare image bind groups for drawing
        let image_bindgroups = self.image_bindgroups();

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.theme.clear_color),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            // Draw lyon elements
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
            rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            for range in indice_ranges {
                rpass.draw_indexed(range, 0, 0..1);
            }

            // Draw images
            rpass.set_pipeline(&self.image_renderer.render_pipeline);
            for (bindgroup, vertex_buf) in image_bindgroups.iter() {
                rpass.set_bind_group(0, bindgroup, &[]);
                rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                rpass.draw_indexed(0..6, 0, 0..1);
            }
        }

        let screen_size = self.screen_size();

        // Draw wgpu brush elements
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

        self.staging_belt.finish();
        self.queue.submit(Some(encoder.finish()));
        frame.present();

        self.staging_belt.recall();
    }

    fn position(&mut self, element_index: usize) -> Rect {
        let screen_size = self.screen_size();
        match &self.elements[element_index].inner {
            Element::TextBox(text_box) => {
                let indent = text_box.indent;
                let pos = (DEFAULT_MARGIN + indent, self.reserved_height);

                let size = text_box.size(
                    &mut self.glyph_brush,
                    pos,
                    (screen_size.0 - pos.0 - DEFAULT_MARGIN, screen_size.1),
                );

                Rect::new(pos, size)
            }
            Element::Spacer(spacer) => Rect::new((0., self.reserved_height), (0., spacer.space)),
            Element::Image(image) => {
                let size = image.size(screen_size);
                match image.is_aligned {
                    Some(Align::Center) => Rect::new(
                        (screen_size.0 / 2. - size.0 / 2., self.reserved_height),
                        size,
                    ),
                    _ => Rect::new((DEFAULT_MARGIN, self.reserved_height), size),
                }
            }
            Element::Table(table) => {
                let pos = (DEFAULT_MARGIN, self.reserved_height);
                let width = table
                    .column_widths(
                        &mut self.glyph_brush,
                        pos,
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                    )
                    .iter()
                    .fold(0., |acc, x| acc + x);
                let height = table
                    .row_heights(
                        &mut self.glyph_brush,
                        pos,
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                    )
                    .iter()
                    .fold(0., |acc, x| acc + x);
                Rect::new(
                    pos,
                    (
                        width * (TABLE_COL_GAP * table.headers.len() as f32),
                        height + (TABLE_ROW_GAP * (table.rows.len() + 1) as f32),
                    ),
                )
            }
        }
    }

    pub fn reposition(&mut self) {
        self.reserved_height = DEFAULT_PADDING;

        for element_index in 0..self.elements.len() {
            let bounds = self.position(element_index);
            self.reserved_height += DEFAULT_PADDING + bounds.size.1;
            self.elements[element_index].bounds = Some(bounds);
        }
    }

    pub fn push(&mut self, mut element: Element) {
        if let Element::Image(ref mut image) = element {
            image.add_callback(self.eventloop_proxy.clone());
        }
        let element_index = self.elements.len();
        self.elements.push(Positioned::new(element));
        let bounds = self.position(element_index);
        self.reserved_height += DEFAULT_PADDING + bounds.size.1;
        self.elements[element_index].bounds = Some(bounds);
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

// Translates points from pixel coordinates to wgpu coordinates
pub fn point(x: f32, y: f32, screen: (f32, f32)) -> [f32; 2] {
    let scale_x = 2. / screen.0;
    let scale_y = 2. / screen.1;
    let new_x = -1. + (x * scale_x);
    let new_y = 1. - (y * scale_y);
    [new_x, new_y]
}
