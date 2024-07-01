use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::color::{native_color, Theme};
use crate::fonts::get_fonts;
use crate::image::ImageRenderer;
use crate::metrics::{histogram, HistTag};
use crate::opts::FontOptions;
use crate::positioner::{Positioned, Positioner, DEFAULT_MARGIN};
use crate::selection::Selection;
use crate::table::TABLE_ROW_GAP;
use crate::text::{CachedTextArea, TextCache, TextSystem};
use crate::utils::{Point, Rect, Size};
use crate::Element;

use anyhow::{Context, Ok};
use bytemuck::{Pod, Zeroable};
use glyphon::{Resolution, SwashCache, TextArea, TextAtlas, TextRenderer};
use lyon::geom::euclid::Point2D;
use lyon::geom::Box2D;
use lyon::path::Polygon;
use lyon::tessellation::*;
use wgpu::util::DeviceExt;
use wgpu::{BindGroup, Buffer, IndexFormat, MultisampleState, TextureFormat};
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
    pub surface_format: TextureFormat,
    pub device: wgpu::Device,
    pub render_pipeline: wgpu::RenderPipeline,
    pub queue: wgpu::Queue,
    pub text_system: TextSystem,
    pub scroll_y: f32,
    pub lyon_buffer: VertexBuffers<Vertex, u16>,
    pub hidpi_scale: f32,
    pub page_width: f32,
    pub image_renderer: ImageRenderer,
    pub theme: Theme,
    pub zoom: f32,
    pub positioner: Positioner,
}

impl Renderer {
    pub const fn screen_height(&self) -> f32 {
        self.positioner.screen_size.1
    }

    pub const fn screen_size(&self) -> Size {
        self.positioner.screen_size
    }

    pub async fn new(
        window: &Window,
        theme: Theme,
        hidpi_scale: f32,
        page_width: f32,
        font_opts: FontOptions,
    ) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
        });
        let surface = unsafe {
            instance
                .create_surface(window)
                .expect("Could not create surface")
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .context("Failed to find an appropriate adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                },
                None,
            )
            .await?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/shader.wgsl"))),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

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
                targets: &[Some(surface_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);
        let image_renderer = ImageRenderer::new(&device, &surface_format);

        let font_system = Arc::new(Mutex::new(get_fonts(&font_opts)));
        let swash_cache = SwashCache::new();
        let mut text_atlas = TextAtlas::new(&device, &queue, surface_format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, &device, MultisampleState::default(), None);
        let text_cache = Arc::new(Mutex::new(TextCache::new()));
        let text_system = TextSystem {
            font_system,
            swash_cache,
            text_renderer,
            text_atlas,
            text_cache,
        };

        let lyon_buffer: VertexBuffers<Vertex, u16> = VertexBuffers::new();

        let positioner = Positioner::new(window.inner_size().into(), hidpi_scale, page_width);
        Ok(Self {
            config,
            surface,
            surface_format,
            device,
            render_pipeline,
            queue,
            text_system,
            scroll_y: 0.,
            lyon_buffer,
            hidpi_scale,
            page_width,
            zoom: 1.,
            image_renderer,
            theme,
            positioner,
        })
    }

    fn draw_scrollbar(&mut self) -> anyhow::Result<()> {
        let (screen_width, screen_height) = self.screen_size();
        if screen_height > self.positioner.reserved_height {
            return Ok(());
        }
        let height = (screen_height / self.positioner.reserved_height) * screen_height;
        self.draw_rectangle(
            Rect::new(
                (
                    screen_width - DEFAULT_MARGIN / 4.,
                    ((self.scroll_y / self.positioner.reserved_height) * screen_height),
                ),
                (DEFAULT_MARGIN / 4., height),
            ),
            [0.3, 0.3, 0.3, 1.0],
        )?;
        Ok(())
    }

    pub fn scrollbar_height(&self) -> f32 {
        (self.screen_height() / self.positioner.reserved_height) * self.screen_height()
    }

    fn render_elements(
        &mut self,
        elements: &[Positioned<Element>],
        selection: &mut Selection,
    ) -> anyhow::Result<Vec<CachedTextArea>> {
        let mut text_areas: Vec<CachedTextArea> = Vec::new();
        let screen_size = self.screen_size();
        for element in elements.iter() {
            let Rect { mut pos, size } =
                element.bounds.as_ref().context("Element not positioned")?;
            let mut scrolled_pos = (pos.0, pos.1 - self.scroll_y);
            // Dont render off screen elements
            if scrolled_pos.1 + size.1 <= 0. {
                continue;
            } else if scrolled_pos.1 >= screen_size.1 {
                break;
            }

            let centering = (screen_size.0 - self.page_width).max(0.) / 2.;

            match &element.inner {
                Element::TextBox(text_box) => {
                    let box_size = text_box.font_size * self.hidpi_scale * self.zoom * 0.75;

                    if text_box.is_checkbox.is_some() {
                        pos.0 += box_size * 1.5;
                        scrolled_pos.0 += box_size * 1.5;
                    }

                    let bounds = (
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN - centering).max(0.),
                        f32::INFINITY,
                    );

                    let areas = text_box.text_areas(
                        &mut self.text_system,
                        pos,
                        bounds,
                        self.zoom,
                        self.scroll_y,
                    );
                    text_areas.push(areas.clone());
                    if text_box.is_code_block || text_box.is_quote_block.is_some() {
                        let color = if let Some(bg_color) = text_box.background_color {
                            bg_color
                        } else {
                            native_color(self.theme.quote_block_color, &self.surface_format)
                        };

                        let mut min = (
                            (scrolled_pos.0 - 10.),
                            scrolled_pos.1 - 5. * self.hidpi_scale * self.zoom,
                        );
                        let max = (
                            min.0
                                + bounds
                                    .0
                                    .max(text_box.size(&mut self.text_system, bounds, self.zoom).0)
                                + 10.,
                            min.1 + size.1 + 12. * self.hidpi_scale * self.zoom,
                        );
                        if let Some(nest) = text_box.is_quote_block {
                            min.0 -= (nest - 1) as f32 * DEFAULT_MARGIN / 2.;
                        }
                        if min.0 < screen_size.0 - DEFAULT_MARGIN - centering {
                            self.draw_rectangle(Rect::from_min_max(min, max), color)?;
                        }
                    }
                    if let Some(nest) = text_box.is_quote_block {
                        for n in 0..nest {
                            let nest_indent = n as f32 * DEFAULT_MARGIN / 2.;
                            let min = (
                                (scrolled_pos.0
                                    - 10.
                                    - 5. * self.hidpi_scale * self.zoom
                                    - nest_indent)
                                    .min(screen_size.0 - DEFAULT_MARGIN - centering),
                                scrolled_pos.1,
                            );
                            let max = (
                                (scrolled_pos.0 - 10. - nest_indent)
                                    .min(screen_size.0 - DEFAULT_MARGIN - centering),
                                min.1 + size.1 + 5. * self.hidpi_scale * self.zoom,
                            );
                            self.draw_rectangle(
                                Rect::from_min_max(min, max),
                                native_color(self.theme.select_color, &self.surface_format),
                            )?;
                        }
                    }
                    if let Some(is_checked) = text_box.is_checkbox {
                        let line_height = text_box.line_height(self.zoom);
                        let min = (
                            scrolled_pos.0 - box_size * 1.5,
                            scrolled_pos.1 + line_height / 2. - box_size / 2.,
                        );
                        let max = (
                            scrolled_pos.0 + box_size - box_size * 1.5,
                            scrolled_pos.1 + line_height / 2. + box_size / 2.,
                        );
                        if max.0 < screen_size.0 - DEFAULT_MARGIN - centering {
                            if is_checked {
                                self.draw_rectangle(
                                    Rect::from_min_max(min, max),
                                    native_color(self.theme.checkbox_color, &self.surface_format),
                                )?;
                                self.draw_tick(
                                    min,
                                    box_size,
                                    native_color(self.theme.text_color, &self.surface_format),
                                    2. * self.hidpi_scale * self.zoom,
                                )?;
                            }
                            self.stroke_rectangle(
                                Rect::from_min_max(min, max),
                                native_color(self.theme.text_color, &self.surface_format),
                                1. * self.hidpi_scale * self.zoom,
                            )?;
                        }
                    }
                    for line in text_box.render_lines(
                        &mut self.text_system,
                        scrolled_pos,
                        bounds,
                        self.zoom,
                        &areas,
                    ) {
                        let min = (line.min.0, line.min.1);
                        let max = (line.max.0, line.max.1 + 2. * self.hidpi_scale * self.zoom);
                        self.draw_rectangle(Rect::from_min_max(min, max), line.color)?;
                    }
                    if let Some(selection_rects) = text_box.render_selection(
                        &mut self.text_system,
                        pos,
                        bounds,
                        self.zoom,
                        selection,
                    ) {
                        for rect in selection_rects {
                            self.draw_rectangle(
                                Rect::from_min_max(
                                    (rect.pos.0, rect.pos.1 - self.scroll_y),
                                    (rect.max().0, rect.max().1 - self.scroll_y),
                                ),
                                native_color(self.theme.select_color, &self.surface_format),
                            )?;
                        }
                    }
                }
                Element::Table(table) => {
                    let bounds = (
                        (screen_size.0 - pos.0 - DEFAULT_MARGIN - centering).max(0.),
                        f32::INFINITY,
                    );
                    let layout = table.layout(
                        &mut self.text_system,
                        &mut self.positioner.taffy,
                        bounds,
                        self.zoom,
                    )?;

                    for (row, node_row) in layout.rows.iter().enumerate() {
                        for (col, node) in node_row.iter().enumerate() {
                            if let Some(row) = table.rows.get(row) {
                                if let Some(text_box) = row.get(col) {
                                    text_areas.push(text_box.text_areas(
                                        &mut self.text_system,
                                        (pos.0 + node.location.x, pos.1 + node.location.y),
                                        (node.size.width, f32::MAX),
                                        self.zoom,
                                        self.scroll_y,
                                    ));

                                    if let Some(selection_rects) = text_box.render_selection(
                                        &mut self.text_system,
                                        (pos.0 + node.location.x, pos.1 + node.location.y),
                                        (node.size.width, node.size.height),
                                        self.zoom,
                                        selection,
                                    ) {
                                        for rect in selection_rects {
                                            self.draw_rectangle(
                                                Rect::from_min_max(
                                                    (rect.pos.0, rect.pos.1 - self.scroll_y),
                                                    (rect.max().0, rect.max().1 - self.scroll_y),
                                                ),
                                                native_color(
                                                    self.theme.select_color,
                                                    &self.surface_format,
                                                ),
                                            )?;
                                        }
                                    }
                                }
                            }
                        }
                        let Some(last_row_node) = node_row.last() else {
                            continue;
                        };
                        let y = last_row_node.location.y
                            + last_row_node.size.height
                            + TABLE_ROW_GAP / 2.;
                        let x = node_row
                            .last()
                            .map(|f| f.location.x + f.size.width)
                            .unwrap_or(0.);
                        {
                            let min = (
                                scrolled_pos.0.max(DEFAULT_MARGIN + centering),
                                scrolled_pos.1 + y,
                            );
                            let max = (
                                scrolled_pos.0 + x,
                                scrolled_pos.1 + y + 1. * self.hidpi_scale * self.zoom,
                            );
                            self.draw_rectangle(
                                Rect::from_min_max(min, max),
                                native_color(self.theme.text_color, &self.surface_format),
                            )?;
                        }
                    }
                }
                Element::Image(_) => {}
                Element::Spacer(spacer) => {
                    if spacer.visible {
                        self.draw_rectangle(
                            Rect::new(
                                (
                                    DEFAULT_MARGIN + centering,
                                    scrolled_pos.1 + size.1 / 2.
                                        - 2. * self.hidpi_scale * self.zoom,
                                ),
                                (
                                    screen_size.0 - 2. * (DEFAULT_MARGIN + centering),
                                    2. * self.hidpi_scale * self.zoom,
                                ),
                            ),
                            native_color(self.theme.text_color, &self.surface_format),
                        )?;
                    }
                }
                Element::Row(row) => {
                    text_areas.append(&mut self.render_elements(&row.elements, selection)?)
                }
                Element::Section(section) => {
                    if let Some(ref summary) = *section.summary {
                        let bounds = summary.bounds.as_ref().unwrap();
                        self.draw_hidden_marker(
                            (
                                bounds.pos.0 - 5. * self.hidpi_scale * self.zoom,
                                bounds.pos.1 + bounds.size.1 / 2. - self.scroll_y,
                            ),
                            10.,
                            native_color(self.theme.text_color, &self.surface_format),
                            *section.hidden.borrow(),
                        )?;
                        text_areas.append(
                            &mut self.render_elements(std::slice::from_ref(summary), selection)?,
                        )
                    }
                    if !*section.hidden.borrow() {
                        text_areas.append(&mut self.render_elements(&section.elements, selection)?)
                    }
                }
            }

            if crate::opts::get_render_element_bounds() {
                let mut rect = element
                    .bounds
                    .as_ref()
                    .context("Element not positioned")?
                    .clone();
                rect.pos.1 -= self.scroll_y;
                let color = glyphon::Color::rgb(255, 0, 255).0;
                let _ = self.stroke_rectangle(rect, native_color(color, &self.surface_format), 1.0);
            }
        }
        self.draw_scrollbar()?;
        Ok(text_areas)
    }

    fn draw_hidden_marker(
        &mut self,
        pos: Point,
        size: f32,
        color: [f32; 4],
        hidden: bool,
    ) -> anyhow::Result<()> {
        let points = if hidden {
            [
                point(pos.0, pos.1, self.screen_size()).into(),
                point(pos.0 - size, pos.1 + size, self.screen_size()).into(),
                point(pos.0 - size, pos.1 - size, self.screen_size()).into(),
            ]
        } else {
            [
                point(pos.0, pos.1 - size / 2., self.screen_size()).into(),
                point(pos.0 - size * 2., pos.1 - size / 2., self.screen_size()).into(),
                point(pos.0 - size, pos.1 + size / 2., self.screen_size()).into(),
            ]
        };
        let triangle = Polygon {
            points: &points,
            closed: true,
        };
        let mut fill_tessellator = FillTessellator::new();
        fill_tessellator.tessellate_polygon(
            triangle,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut self.lyon_buffer, |vertex: FillVertex| Vertex {
                pos: [vertex.position().x, vertex.position().y, 0.0],
                color,
            }),
        )?;
        Ok(())
    }

    fn draw_rectangle(&mut self, rect: Rect, color: [f32; 4]) -> anyhow::Result<()> {
        let min = point(rect.pos.0, rect.pos.1, self.screen_size());
        let max = point(rect.max().0, rect.max().1, self.screen_size());
        let mut fill_tessellator = FillTessellator::new();
        fill_tessellator.tessellate_rectangle(
            &Box2D::new(Point2D::from(min), Point2D::from(max)),
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut self.lyon_buffer, |vertex: FillVertex| Vertex {
                pos: [vertex.position().x, vertex.position().y, 0.0],
                color,
            }),
        )?;
        Ok(())
    }

    fn stroke_rectangle(&mut self, rect: Rect, color: [f32; 4], width: f32) -> anyhow::Result<()> {
        let mut stroke_tessellator = StrokeTessellator::new();
        let screen_size = self.screen_size();
        stroke_tessellator.tessellate_rectangle(
            &Box2D::new(Point2D::from(rect.pos), Point2D::from(rect.max())),
            &StrokeOptions::default().with_line_width(width),
            &mut BuffersBuilder::new(&mut self.lyon_buffer, |vertex: StrokeVertex| {
                let point = point(vertex.position().x, vertex.position().y, screen_size);
                Vertex {
                    pos: [point[0], point[1], 0.0],
                    color,
                }
            }),
        )?;
        Ok(())
    }

    fn draw_tick(
        &mut self,
        pos: Point,
        box_size: f32,
        color: [f32; 4],
        width: f32,
    ) -> anyhow::Result<()> {
        let screen_size = self.screen_size();
        let mut stroke_tessellator = StrokeTessellator::new();
        let stroke_opts = StrokeOptions::default().with_line_width(width);
        let mut vertex_builder =
            BuffersBuilder::new(&mut self.lyon_buffer, |vertex: StrokeVertex| {
                let point = point(vertex.position().x, vertex.position().y, screen_size);
                Vertex {
                    pos: [point[0], point[1], 0.0],
                    color,
                }
            });
        let mut builder = stroke_tessellator.builder(&stroke_opts, &mut vertex_builder);

        // Build a simple path.
        builder.begin((pos.0 + box_size * 0.2, pos.1 + box_size * 0.5).into());
        builder.line_to((pos.0 + box_size * 0.4, pos.1 + box_size * 0.7).into());
        builder.line_to((pos.0 + box_size * 0.8, pos.1 + box_size * 0.2).into());
        builder.end(false);
        builder.build()?;
        Ok(())
    }

    fn image_bindgroups(
        &mut self,
        elements: &mut [Positioned<Element>],
    ) -> Vec<(Arc<BindGroup>, Buffer)> {
        let screen_size = self.screen_size();
        let mut bind_groups = Vec::new();
        for element in elements.iter_mut() {
            let Rect { pos, size } = element.bounds.as_ref().unwrap();
            let pos = (pos.0, pos.1 - self.scroll_y);
            if pos.1 + size.1 <= 0. {
                continue;
            } else if pos.1 >= screen_size.1 {
                break;
            }
            match &mut element.inner {
                Element::Image(ref mut image) => {
                    if let Some(bind_group) = image.bind_group.clone().or_else(|| {
                        image.create_bind_group(
                            &self.device,
                            &self.queue,
                            &self.image_renderer.sampler,
                            &self.image_renderer.bindgroup_layout,
                        )
                    }) {
                        let vertex_buf =
                            ImageRenderer::vertex_buf(&self.device, pos, *size, screen_size);
                        bind_groups.push((bind_group.clone(), vertex_buf));
                    }
                }
                Element::Row(ref mut row) => {
                    for element in row.elements.iter_mut() {
                        let Rect { pos, size } = element.bounds.as_ref().unwrap();
                        let pos = (pos.0, pos.1 - self.scroll_y);
                        if let Element::Image(ref mut image) = &mut element.inner {
                            if let Some(bind_group) = image.bind_group.clone().or_else(|| {
                                image.create_bind_group(
                                    &self.device,
                                    &self.queue,
                                    &self.image_renderer.sampler,
                                    &self.image_renderer.bindgroup_layout,
                                )
                            }) {
                                let vertex_buf = ImageRenderer::vertex_buf(
                                    &self.device,
                                    pos,
                                    *size,
                                    screen_size,
                                );
                                bind_groups.push((bind_group.clone(), vertex_buf));
                            }
                        }
                    }
                }
                Element::Section(ref mut section) => {
                    if *section.hidden.borrow() {
                        continue;
                    }
                    for element in section.elements.iter_mut() {
                        let Rect { pos, size } = element.bounds.as_ref().unwrap();
                        let pos = (pos.0, pos.1 - self.scroll_y);
                        if let Element::Image(ref mut image) = &mut element.inner {
                            if let Some(bind_group) = image.bind_group.clone().or_else(|| {
                                image.create_bind_group(
                                    &self.device,
                                    &self.queue,
                                    &self.image_renderer.sampler,
                                    &self.image_renderer.bindgroup_layout,
                                )
                            }) {
                                let vertex_buf = ImageRenderer::vertex_buf(
                                    &self.device,
                                    pos,
                                    *size,
                                    screen_size,
                                );
                                bind_groups.push((bind_group.clone(), vertex_buf));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        bind_groups
    }

    pub fn redraw(
        &mut self,
        elements: &mut [Positioned<Element>],
        selection: &mut Selection,
    ) -> anyhow::Result<()> {
        selection.text.clear();
        let frame = self
            .surface
            .get_current_texture()
            .context("Failed to acquire next swap chain texture")?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Prepare and render elements that use lyon
        self.lyon_buffer.indices.clear();
        self.lyon_buffer.vertices.clear();
        let cached_text_areas = self.render_elements(elements, selection)?;
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
            let mut text_cache = self.text_system.text_cache.lock().unwrap();
            let text_areas: Vec<TextArea> = cached_text_areas
                .iter()
                .map(|c| c.text_area(&text_cache))
                .collect();

            self.text_system.text_renderer.prepare(
                &self.device,
                &self.queue,
                &mut self.text_system.font_system.lock().unwrap(),
                &mut self.text_system.text_atlas,
                Resolution {
                    width: self.config.width,
                    height: self.config.height,
                },
                text_areas,
                &mut self.text_system.swash_cache,
            )?;
            text_cache.trim();
        }

        {
            let background_color = {
                let c = native_color(self.theme.background_color, &self.surface_format);
                wgpu::Color {
                    r: c[0] as f64,
                    g: c[1] as f64,
                    b: c[2] as f64,
                    a: c[3] as f64,
                }
            };
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(background_color),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            // Draw lyon elements
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
            rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..self.lyon_buffer.indices.len() as u32, 0, 0..1);

            // Draw images
            rpass.set_pipeline(&self.image_renderer.render_pipeline);
            rpass.set_index_buffer(self.image_renderer.index_buf.slice(..), IndexFormat::Uint16);
            for (bindgroup, vertex_buf) in image_bindgroups.iter() {
                rpass.set_bind_group(0, bindgroup, &[]);
                rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                rpass.draw_indexed(0..6, 0, 0..1);
            }

            self.text_system
                .text_renderer
                .render(&self.text_system.text_atlas, &mut rpass)
                .unwrap();
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.text_system.text_atlas.trim();

        Ok(())
    }

    pub fn reposition(&mut self, elements: &mut [Positioned<Element>]) -> anyhow::Result<()> {
        let start = Instant::now();
        let res = self
            .positioner
            .reposition(&mut self.text_system, elements, self.zoom);
        histogram!(HistTag::Reposition).record(start.elapsed());
        res
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        self.scroll_y = scroll_y.clamp(
            0.,
            (self.positioner.reserved_height - self.screen_height()).max(0.),
        )
    }
}

// Translates points from pixel coordinates to wgpu coordinates
pub fn point(x: f32, y: f32, screen: Size) -> [f32; 2] {
    let scale_x = 2. / screen.0;
    let scale_y = 2. / screen.1;
    let new_x = -1. + (x * scale_x);
    let new_y = 1. - (y * scale_y);
    [new_x, new_y]
}
