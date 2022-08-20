use crate::color::Theme;
use crate::fonts;
use crate::image::ImageRenderer;
use crate::opts::FontOptions;
use crate::positioner::{Positioned, Positioner, DEFAULT_MARGIN};
use crate::table::{TABLE_COL_GAP, TABLE_ROW_GAP};
use crate::utils::Rect;
use crate::{Element, InlyneEvent};
use anyhow::Ok;
use bytemuck::{Pod, Zeroable};
use lyon::geom::euclid::Point2D;
use lyon::geom::Box2D;
use lyon::tessellation::*;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::{util::StagingBelt, TextureFormat};
use wgpu::{BindGroup, Buffer, IndexFormat};
use wgpu_glyph::{GlyphBrush, GlyphBrushBuilder};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

pub struct Renderer {
    pub config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub render_pipeline: wgpu::RenderPipeline,
    pub queue: wgpu::Queue,
    pub glyph_brush: GlyphBrush<()>,
    pub staging_belt: StagingBelt,
    pub scroll_y: f32,
    pub lyon_buffer: VertexBuffers<Vertex, u16>,
    pub hidpi_scale: f32,
    pub image_renderer: ImageRenderer,
    pub eventloop_proxy: EventLoopProxy<InlyneEvent>,
    pub theme: Theme,
    pub selection: Option<((f32, f32), (f32, f32))>,
    pub selection_text: String,
    pub zoom: f32,
    pub positioner: Positioner,
}

impl Renderer {
    pub const fn screen_height(&self) -> f32 {
        self.positioner.screen_size.1
    }

    pub const fn screen_size(&self) -> (f32, f32) {
        self.positioner.screen_size
    }

    pub async fn new(
        window: &Window,
        eventloop_proxy: EventLoopProxy<InlyneEvent>,
        theme: Theme,
        hidpi_scale: f32,
        font_opts: FontOptions,
    ) -> anyhow::Result<Self> {
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

        let glyph_brush = GlyphBrushBuilder::using_fonts(fonts::get_fonts(&font_opts)?)
            .build(&device, swapchain_format);

        let lyon_buffer: VertexBuffers<Vertex, u16> = VertexBuffers::new();

        let positioner = Positioner::new(window.inner_size().into(), hidpi_scale);
        Ok(Self {
            config,
            surface,
            device,
            render_pipeline,
            queue,
            glyph_brush,
            staging_belt,
            scroll_y: 0.,
            lyon_buffer,
            hidpi_scale,
            zoom: 1.,
            image_renderer,
            eventloop_proxy,
            theme,
            selection: None,
            selection_text: String::new(),
            positioner,
        })
    }

    fn draw_scrollbar(&mut self) -> u32 {
        let screen_height = self.screen_height();
        let screen_width = self.config.width as f32;
        let top = if screen_height < self.positioner.reserved_height {
            1. - (self.scroll_y / (self.positioner.reserved_height - screen_height)
                * (1. - (screen_height / self.positioner.reserved_height)))
                * 2.
        } else {
            1.
        };
        let bottom = if screen_height < self.positioner.reserved_height {
            top - ((screen_height / self.positioner.reserved_height) * 2.)
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

    fn render_elements(
        &mut self,
        elements: &[Positioned<Element>],
    ) -> anyhow::Result<Vec<Range<u32>>> {
        let mut indice_ranges = Vec::new();
        let mut _prev_indice_num = 0;
        let screen_size = self.screen_size();
        for element in elements.iter() {
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

            match &element.inner {
                Element::TextBox(text_box) => {
                    let bounds = (screen_size.0 - pos.0 - DEFAULT_MARGIN, screen_size.1);
                    self.glyph_brush
                        .queue(&text_box.glyph_section(*pos, bounds, self.zoom));
                    if text_box.is_code_block || text_box.is_quote_block.is_some() {
                        let color = if let Some(bg_color) = text_box.background_color {
                            bg_color
                        } else if text_box.is_code_block {
                            self.theme.code_block_color
                        } else {
                            self.theme.quote_block_color
                        };

                        let mut min = (
                            (scrolled_pos.0 - 10. * self.hidpi_scale * self.zoom)
                                .min(screen_size.0 - DEFAULT_MARGIN),
                            scrolled_pos.1,
                        );
                        let max = (
                            (min.0 + bounds.0 + 10. * self.hidpi_scale * self.zoom)
                                .min(screen_size.0 - DEFAULT_MARGIN + 10.),
                            min.1 + size.1 + 5. * self.hidpi_scale * self.zoom,
                        );
                        if let Some(nest) = text_box.is_quote_block {
                            min.0 -= (nest - 1) as f32 * DEFAULT_MARGIN / 2.;
                        }
                        indice_ranges
                            .push(self.draw_rectangle(Rect::from_min_max(min, max), color)?);
                    }
                    if let Some(nest) = text_box.is_quote_block {
                        for n in 0..nest {
                            let nest_indent = n as f32 * DEFAULT_MARGIN / 2.;
                            let min = (
                                (scrolled_pos.0 - 20. - nest_indent)
                                    .min(screen_size.0 - DEFAULT_MARGIN),
                                scrolled_pos.1,
                            );
                            let max = (
                                (scrolled_pos.0 - 10. - nest_indent)
                                    .min(screen_size.0 - DEFAULT_MARGIN),
                                min.1 + size.1 + 5.,
                            );
                            indice_ranges.push(self.draw_rectangle(
                                Rect::from_min_max(min, max),
                                self.theme.select_color,
                            )?);
                        }
                    }
                    if let Some(ref lines) = text_box.render_lines(
                        &mut self.glyph_brush,
                        scrolled_pos,
                        bounds,
                        self.zoom,
                    ) {
                        for line in lines {
                            let min = (
                                line.0 .0.min(screen_size.0 - DEFAULT_MARGIN).max(pos.0),
                                line.0 .1,
                            );
                            let max = (
                                line.1 .0.min(screen_size.0 - DEFAULT_MARGIN).max(pos.0),
                                line.1 .1 + 2.,
                            );
                            indice_ranges.push(self.draw_rectangle(
                                Rect::from_min_max(min, max),
                                self.theme.text_color,
                            )?);
                        }
                    }
                    if let Some(selection) = self.selection {
                        let (selection_rects, selection_text) = text_box.render_selection(
                            &mut self.glyph_brush,
                            *pos,
                            bounds,
                            self.zoom,
                            selection,
                        );
                        self.selection_text.push_str(&selection_text);
                        for rect in selection_rects {
                            indice_ranges.push(self.draw_rectangle(
                                Rect::from_min_max(
                                    (rect.pos.0, rect.pos.1 - self.scroll_y),
                                    (rect.max.0, rect.max.1 - self.scroll_y),
                                ),
                                self.theme.select_color,
                            )?);
                        }
                    }
                }
                Element::Table(table) => {
                    let row_heights = table.row_heights(
                        &mut self.glyph_brush,
                        *pos,
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                        self.zoom,
                    );
                    let column_widths = table.column_widths(
                        &mut self.glyph_brush,
                        *pos,
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN, f32::INFINITY),
                        self.zoom,
                    );
                    let mut x = 0.;
                    let mut y = 0.;

                    let header_height = row_heights.first().unwrap();
                    for (col, width) in column_widths.iter().enumerate() {
                        let text_box = table.headers.get(col).unwrap();
                        let bounds = (screen_size.0 - pos.0 - x - DEFAULT_MARGIN, f32::INFINITY);
                        self.glyph_brush.queue(&text_box.glyph_section(
                            (pos.0 + x, pos.1 + y),
                            bounds,
                            self.zoom,
                        ));
                        if let Some(selection) = self.selection {
                            let (selection_rects, selection_text) = text_box.render_selection(
                                &mut self.glyph_brush,
                                (pos.0 + x, pos.1 + y),
                                bounds,
                                self.zoom,
                                selection,
                            );
                            self.selection_text.push_str(&selection_text);
                            for rect in selection_rects {
                                indice_ranges.push(self.draw_rectangle(
                                    Rect::from_min_max(
                                        (rect.pos.0, rect.pos.1 - self.scroll_y),
                                        (rect.max.0, rect.max.1 - self.scroll_y),
                                    ),
                                    self.theme.select_color,
                                )?);
                            }
                        }
                        x += width + TABLE_COL_GAP;
                    }
                    y += header_height + (TABLE_ROW_GAP / 2.);
                    {
                        let min = (
                            scrolled_pos.0.min(screen_size.0 - DEFAULT_MARGIN),
                            scrolled_pos.1 + y,
                        );
                        let max = (
                            scrolled_pos.0
                                + x.max(scrolled_pos.0).min(screen_size.0 - DEFAULT_MARGIN),
                            scrolled_pos.1 + y + 3.,
                        );
                        indice_ranges.push(
                            self.draw_rectangle(
                                Rect::from_min_max(min, max),
                                self.theme.text_color,
                            )?,
                        );
                    }

                    y += TABLE_ROW_GAP / 2.;
                    for (row, height) in row_heights.iter().skip(1).enumerate() {
                        let mut x = 0.;
                        for (col, width) in column_widths.iter().enumerate() {
                            if let Some(row) = table.rows.get(row) {
                                if let Some(text_box) = row.get(col) {
                                    let bounds =
                                        (screen_size.0 - pos.0 - x - DEFAULT_MARGIN, f32::INFINITY);
                                    self.glyph_brush.queue(&text_box.glyph_section(
                                        (pos.0 + x, pos.1 + y),
                                        bounds,
                                        self.zoom,
                                    ));

                                    if let Some(selection) = self.selection {
                                        let (selection_rects, selection_text) = text_box
                                            .render_selection(
                                                &mut self.glyph_brush,
                                                (pos.0 + x, pos.1 + y),
                                                bounds,
                                                self.zoom,
                                                selection,
                                            );
                                        self.selection_text.push_str(&selection_text);
                                        for rect in selection_rects {
                                            indice_ranges.push(self.draw_rectangle(
                                                Rect::from_min_max(
                                                    (rect.pos.0, rect.pos.1 - self.scroll_y),
                                                    (rect.max.0, rect.max.1 - self.scroll_y),
                                                ),
                                                self.theme.select_color,
                                            )?);
                                        }
                                    }
                                }
                            }
                            x += width + TABLE_COL_GAP;
                        }
                        y += height + (TABLE_COL_GAP / 2.);
                        {
                            let min = (
                                scrolled_pos.0.min(screen_size.0 - DEFAULT_MARGIN),
                                scrolled_pos.1 + y,
                            );
                            let max = (
                                (scrolled_pos.0 + x)
                                    .max(scrolled_pos.0)
                                    .min(screen_size.0 - DEFAULT_MARGIN),
                                scrolled_pos.1 + y + 3.,
                            );
                            let color = self.theme.code_block_color;
                            indice_ranges
                                .push(self.draw_rectangle(Rect::from_min_max(min, max), color)?);
                        }
                        y += TABLE_ROW_GAP / 2.;
                    }
                }
                Element::Image(_) => {}
                Element::Spacer(_) => {}
            }
        }

        indice_ranges.push(_prev_indice_num..self.draw_scrollbar());
        Ok(indice_ranges)
    }

    fn draw_rectangle(&mut self, rect: Rect, color: [f32; 4]) -> anyhow::Result<Range<u32>> {
        let prev_indice_num = self.lyon_buffer.indices.len() as u32;
        let min = point(rect.pos.0, rect.pos.1, self.screen_size());
        let max = point(rect.max.0, rect.max.1, self.screen_size());
        let mut fill_tessellator = FillTessellator::new();
        fill_tessellator.tessellate_rectangle(
            &Box2D::new(Point2D::from(min), Point2D::from(max)),
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut self.lyon_buffer, |vertex: FillVertex| Vertex {
                pos: [vertex.position().x, vertex.position().y, 0.0],
                color,
            }),
        )?;
        Ok(prev_indice_num..self.lyon_buffer.indices.len() as u32)
    }

    fn image_bindgroups(
        &mut self,
        elements: &mut [Positioned<Element>],
    ) -> Vec<(Arc<BindGroup>, Buffer)> {
        let screen_size = self.screen_size();
        let mut bind_groups = Vec::new();
        for element in elements.iter_mut() {
            let Rect { pos, size, max } = element.bounds.as_ref().unwrap();
            let pos = (pos.0, pos.1 - self.scroll_y);
            if max.1 <= 0. {
                continue;
            } else if pos.1 >= screen_size.1 {
                break;
            }
            if let Element::Image(ref mut image) = &mut element.inner {
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

    pub fn redraw(&mut self, elements: &mut [Positioned<Element>]) -> anyhow::Result<()> {
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
        let indice_ranges = self.render_elements(elements)?;
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
        let image_bindgroups = self.image_bindgroups(elements);

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.theme.background_color),
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
            rpass.set_index_buffer(self.image_renderer.index_buf.slice(..), IndexFormat::Uint16);
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
        Ok(())
    }

    pub fn reposition(&mut self, elements: &mut [Positioned<Element>]) {
        self.positioner
            .reposition(&mut self.glyph_brush, elements, self.zoom);
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        if self.positioner.reserved_height > self.screen_height() {
            self.scroll_y = scroll_y
                .max(0.)
                .min(self.positioner.reserved_height - self.screen_height());
        }
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
