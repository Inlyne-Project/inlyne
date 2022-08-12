pub mod color;
pub mod image;
pub mod renderer;
pub mod table;
pub mod text;
pub mod utils;

use crate::image::Image;
use crate::image::ImageSize;
use crate::table::Table;

use color::Theme;
use renderer::{Renderer, Spacer};
use utils::{Align, Rect};

use anyhow::Context;
use comrak::{markdown_to_html, ComrakOptions};
use copypasta::{ClipboardContext, ClipboardProvider};
use html5ever::local_name;
use html5ever::tendril::*;
use html5ever::tokenizer::TagToken;
use html5ever::tokenizer::{BufferQueue, TagKind};
use html5ever::tokenizer::{Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts};
use text::{Text, TextBox};
use winit::event::ModifiersState;
use winit::event::VirtualKeyCode;
use winit::event::{ElementState, MouseButton};
use winit::{
    event::{Event, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{CursorIcon, Window},
};
use Token::{CharacterTokens, EOFToken};

use std::collections::VecDeque;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

use crate::renderer::DEFAULT_MARGIN;

#[derive(Debug)]
pub enum InlyneEvent {
    Reposition,
}

pub enum Element {
    TextBox(TextBox),
    Spacer(Spacer),
    Image(Image),
    Table(Table),
}

impl From<Image> for Element {
    fn from(image: Image) -> Self {
        Element::Image(image)
    }
}

impl From<Spacer> for Element {
    fn from(spacer: Spacer) -> Self {
        Element::Spacer(spacer)
    }
}

impl From<TextBox> for Element {
    fn from(text_box: TextBox) -> Self {
        Element::TextBox(text_box)
    }
}

impl From<Table> for Element {
    fn from(table: Table) -> Self {
        Element::Table(table)
    }
}

pub struct Inlyne {
    window: Arc<Window>,
    event_loop: EventLoop<InlyneEvent>,
    renderer: Renderer,
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    clipboard: ClipboardContext,
}

impl Inlyne {
    pub async fn new(theme: Theme, scale: Option<f32>) -> Self {
        let event_loop = EventLoop::<InlyneEvent>::with_user_event();
        let window = Arc::new(Window::new(&event_loop).unwrap());
        window.set_title("Inlyne");
        let renderer = Renderer::new(&window, event_loop.create_proxy(), theme, scale.unwrap_or(window.scale_factor() as f32)).await;
        let clipboard = ClipboardContext::new().unwrap();

        Self {
            window,
            event_loop,
            renderer,
            element_queue: Arc::new(Mutex::new(VecDeque::new())),
            clipboard,
        }
    }

    pub fn push<T: Into<Element>>(&mut self, element: T) {
        let element = element.into();
        self.renderer.push(element);
    }

    pub fn run(mut self) {
        let mut click_scheduled = false;
        let mut scrollbar_held = false;
        let mut mouse_down = false;
        let mut modifiers = ModifiersState::empty();
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::Reposition => {
                        self.renderer.reposition();
                        self.window.request_redraw()
                    }
                },
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    self.renderer.config.width = size.width;
                    self.renderer.config.height = size.height;
                    self.renderer
                        .surface
                        .configure(&self.renderer.device, &self.renderer.config);
                    self.renderer.reposition();
                    self.window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    let queued_elements =
                        if let Ok(mut element_queue) = self.element_queue.try_lock() {
                            Some(element_queue.drain(0..).collect::<Vec<Element>>())
                        } else {
                            None
                        };
                    if let Some(queue) = queued_elements {
                        for element in queue {
                            self.renderer.push(element);
                        }
                    }
                    self.renderer.redraw();
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::MouseWheel { delta, .. } => match delta {
                        MouseScrollDelta::PixelDelta(pos) => {
                            {
                                let screen_height = self.renderer.screen_height();
                                if self.renderer.reserved_height > screen_height {
                                    self.renderer.scroll_y -= pos.y as f32;

                                    if self.renderer.scroll_y.is_sign_negative() {
                                        self.renderer.scroll_y = 0.;
                                    } else if self.renderer.scroll_y
                                        >= (self.renderer.reserved_height - screen_height)
                                    {
                                        self.renderer.scroll_y =
                                            self.renderer.reserved_height - screen_height;
                                    }
                                }
                            }
                            self.window.request_redraw();
                        }
                        MouseScrollDelta::LineDelta(_, y_delta) => {
                            {
                                let screen_height = self.renderer.screen_height();
                                if self.renderer.reserved_height > screen_height {
                                    self.renderer.scroll_y -= y_delta;

                                    if self.renderer.scroll_y.is_sign_negative() {
                                        self.renderer.scroll_y = 0.;
                                    } else if self.renderer.scroll_y
                                        >= (self.renderer.reserved_height - screen_height)
                                    {
                                        self.renderer.scroll_y =
                                            self.renderer.reserved_height - screen_height;
                                    }
                                }
                            }
                            self.window.request_redraw();
                        }
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        let mut over_link = false;
                        let screen_size = self.renderer.screen_size();
                        let loc = (
                            position.x as f32,
                            position.y as f32 + self.renderer.scroll_y,
                        );
                        for element in self.renderer.elements.iter() {
                            if element.contains(loc) {
                                match element.deref() {
                                    Element::TextBox(ref text_box) => {
                                        let bounds = element.bounds.as_ref().unwrap();
                                        let cursor = text_box.hovering_over(
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            click_scheduled,
                                        );
                                        self.window.set_cursor_icon(cursor);
                                        over_link = true;
                                        break;
                                    }
                                    Element::Table(ref table) => {
                                        let bounds = element.bounds.as_ref().unwrap();
                                        let cursor = table.hovering_over(
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            click_scheduled,
                                        );
                                        self.window.set_cursor_icon(cursor);
                                        over_link = true;
                                        break;
                                    }
                                    Element::Image(image) => {
                                        if let Some(ref link) = image.is_link {
                                            if click_scheduled && open::that(link).is_err() {
                                                eprintln!("Could not open link");
                                            }
                                            self.window.set_cursor_icon(CursorIcon::Hand);
                                            over_link = true;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if scrollbar_held
                            || (Rect::new(
                                (screen_size.0 * (49. / 50.), 0.),
                                (screen_size.0 * (1. / 50.), screen_size.1),
                            )
                            .contains(position.into())
                                && click_scheduled)
                        {
                            let target_scroll = ((position.y as f32 / screen_size.1)
                                * self.renderer.reserved_height)
                                - (screen_size.1 / self.renderer.reserved_height * screen_size.1);
                            self.renderer.scroll_y = if target_scroll <= 0. {
                                0.
                            } else if target_scroll >= self.renderer.reserved_height - screen_size.1
                            {
                                self.renderer.reserved_height - screen_size.1
                            } else {
                                target_scroll
                            };
                            self.window.request_redraw();
                            if !scrollbar_held {
                                scrollbar_held = true;
                            }
                        } else {
                            if click_scheduled {
                                self.renderer.selection = Some((loc, loc));
                            }
                            if let Some(ref mut selection) = self.renderer.selection {
                                if mouse_down {
                                    selection.1 = loc;
                                    self.window.request_redraw();
                                }
                            }
                        }

                        if !over_link {
                            self.window.set_cursor_icon(CursorIcon::Default);
                        }
                        click_scheduled = false;
                    }
                    WindowEvent::MouseInput {
                        state,
                        button: MouseButton::Left,
                        ..
                    } => match state {
                        ElementState::Pressed => {
                            self.renderer.selection = None;
                            mouse_down = true;
                            click_scheduled = true;
                            self.window.request_redraw();
                        }
                        ElementState::Released => {
                            click_scheduled = false;
                            scrollbar_held = false;
                            mouse_down = false;
                        }
                    },
                    WindowEvent::ModifiersChanged(modifier_state) => modifiers = modifier_state,
                    WindowEvent::KeyboardInput { input, .. } => {
                        if input.virtual_keycode == Some(VirtualKeyCode::C) {
                            let copy = (cfg!(target_os = "macos") && modifiers.logo())
                                || (!cfg!(target_os = "macos") && modifiers.ctrl());
                            if copy {
                                self.clipboard
                                    .set_contents(self.renderer.selection_text.trim().to_owned())
                                    .unwrap()
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        });
    }
}

pub enum ListType {
    Ordered(usize),
    Unordered,
}

struct Header(f32);
struct TokenPrinter {
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    current_textbox: TextBox,
    is_link: Option<String>,
    is_header: Option<Header>,
    is_code: bool,
    is_list_item: bool,
    list_type: Option<ListType>,
    is_bold: bool,
    is_italic: bool,
    is_underlined: bool,
    is_striked: bool,
    is_pre_formated: bool,
    global_indent: f32,
    align: Option<Align>,
    text_align: Option<Align>,
    is_table: Option<Table>,
    is_table_row: Option<Vec<TextBox>>,
    is_table_header: Option<TextBox>,
    is_table_data: Option<TextBox>,
    is_small: bool,
    hidpi_scale: f32,
    theme: Theme,
    window: Arc<Window>,
}

impl TokenPrinter {
    fn push_current_textbox(&mut self) {
        if !self.current_textbox.texts.is_empty() {
            let mut empty = true;
            for text in &self.current_textbox.texts {
                if !text.text.trim().is_empty() {
                    empty = false;
                    break;
                }
            }
            if !empty {
                self.push_element(self.current_textbox.clone().into());
            }
            self.current_textbox = TextBox::new(Vec::new(), self.hidpi_scale, &self.theme);
        }
    }
    fn push_spacer(&mut self) {
        self.push_element(Spacer::new(10.).into());
    }
    fn push_element(&mut self, element: Element) {
        self.element_queue.lock().unwrap().push_back(element);
        self.window.request_redraw()
    }
}
impl TokenSink for TokenPrinter {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            TagToken(tag) => {
                let tag_name = tag.name.to_string();
                match tag.kind {
                    TagKind::StartTag => match tag_name.as_str() {
                        "th" => {
                            self.is_table_header =
                                Some(TextBox::new(Vec::new(), self.hidpi_scale, &self.theme))
                        }
                        "td" => {
                            self.is_table_data =
                                Some(TextBox::new(Vec::new(), self.hidpi_scale, &self.theme))
                        }
                        "table" => {
                            self.is_table = Some(Table::new());
                            self.push_spacer();
                        }
                        "a" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("href") {
                                    self.is_link = Some(attr.value.to_string());
                                    break;
                                }
                            }
                        }
                        "small" => self.is_small = true,
                        "br" => {
                            self.push_current_textbox();
                        }
                        "ins" | "u" => self.is_underlined = true,
                        "del" | "s" => self.is_striked = true,
                        "img" => {
                            let attrs = tag.attrs;
                            let mut local_align = None;
                            let mut size = None;
                            for attr in &attrs {
                                match attr.name.local {
                                    local_name!("align") => match attr.value.to_string().as_str() {
                                        "center" => local_align = Some(Align::Center),
                                        "left" => local_align = Some(Align::Left),
                                        _ => {}
                                    },
                                    local_name!("width") => {
                                        if let Ok(px_width) = attr.value.parse::<u32>() {
                                            size = Some(ImageSize::PxWidth(px_width));
                                        }
                                    }
                                    local_name!("height") => {
                                        if let Ok(px_height) = attr.value.parse::<u32>() {
                                            size = Some(ImageSize::PxHeight(px_height));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            for attr in attrs {
                                if attr.name.local == local_name!("src") {
                                    let align = self.align.as_ref().unwrap_or(&Align::Left);
                                    let mut image =
                                        Image::from_url(attr.value.to_string(), self.hidpi_scale)
                                            .with_align(
                                                local_align.unwrap_or_else(|| align.clone()),
                                            );
                                    if let Some(ref link) = self.is_link {
                                        image.set_link(link.clone())
                                    }
                                    if let Some(size) = size {
                                        image = image.with_size(size);
                                    }

                                    self.push_element(image.into());
                                    self.push_spacer();
                                    break;
                                }
                            }
                        }
                        "p" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "left" => self.align = Some(Align::Left),
                                        "center" => self.align = Some(Align::Center),
                                        "right" => self.text_align = Some(Align::Right),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "left" => self.text_align = Some(Align::Left),
                                        "center" => self.text_align = Some(Align::Center),
                                        "right" => self.text_align = Some(Align::Right),
                                        _ => {}
                                    }
                                }
                            }
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "em" | "i" => self.is_italic = true,
                        "bold" | "strong" => self.is_bold = true,
                        "code" => self.is_code = true,
                        "li" => {
                            self.current_textbox.indent = self.global_indent;
                            self.is_list_item = true
                        }
                        "ul" => {
                            self.push_current_textbox();
                            self.global_indent += DEFAULT_MARGIN / 4.;
                            self.list_type = Some(ListType::Unordered);
                        }
                        "ol" => {
                            let mut start_index = 1;
                            for attr in tag.attrs {
                                if attr.name.local == local_name!("start") {
                                    start_index = attr.value.parse::<usize>().unwrap();
                                }
                            }
                            self.push_current_textbox();
                            self.global_indent += DEFAULT_MARGIN / 4.;
                            self.current_textbox.indent = self.global_indent;
                            self.list_type = Some(ListType::Ordered(start_index));
                        }
                        "h1" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "center" => self.align = Some(Align::Center),
                                        "left" => self.align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "right" => self.text_align = Some(Align::Right),
                                        "center" => self.text_align = Some(Align::Center),
                                        "left" => self.text_align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                            }
                            self.is_underlined = true;
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = Some(Header(32.));
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "h2" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "center" => self.align = Some(Align::Center),
                                        "left" => self.align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "right" => self.text_align = Some(Align::Right),
                                        "center" => self.text_align = Some(Align::Center),
                                        "left" => self.text_align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                            }
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = Some(Header(24.));
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "h3" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "center" => self.align = Some(Align::Center),
                                        "left" => self.align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "right" => self.text_align = Some(Align::Right),
                                        "center" => self.text_align = Some(Align::Center),
                                        "left" => self.text_align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                            }
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = Some(Header(18.72));
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "h4" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "center" => self.align = Some(Align::Center),
                                        "left" => self.align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "right" => self.text_align = Some(Align::Right),
                                        "center" => self.text_align = Some(Align::Center),
                                        "left" => self.text_align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                            }
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = Some(Header(16.));
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "h5" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "center" => self.align = Some(Align::Center),
                                        "left" => self.align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "right" => self.text_align = Some(Align::Right),
                                        "center" => self.text_align = Some(Align::Center),
                                        "left" => self.text_align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                            }
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = Some(Header(13.28));
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "h6" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("align") {
                                    match attr.value.to_string().as_str() {
                                        "center" => self.align = Some(Align::Center),
                                        "left" => self.align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                                if attr.name.local == *"text-align" {
                                    match attr.value.to_string().as_str() {
                                        "right" => self.text_align = Some(Align::Right),
                                        "center" => self.text_align = Some(Align::Center),
                                        "left" => self.text_align = Some(Align::Left),
                                        _ => {}
                                    }
                                }
                            }
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = Some(Header(10.72));
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "pre" => {
                            self.push_current_textbox();
                            self.current_textbox.set_code_block(true);
                            self.is_pre_formated = true
                        }
                        "tr" => {
                            self.is_table_row = Some(Vec::new());
                        }
                        _ => {}
                    },
                    TagKind::EndTag => match tag_name.as_str() {
                        "ins" | "u" => self.is_underlined = false,
                        "del" | "s" => self.is_striked = false,
                        "small" => self.is_small = false,
                        "th" => {
                            let table_header = self.is_table_header.take().unwrap();
                            self.is_table.as_mut().unwrap().push_header(table_header);
                        }
                        "td" => {
                            let table_data = self.is_table_data.take().unwrap();
                            self.is_table_row.as_mut().unwrap().push(table_data);
                        }
                        "tr" => {
                            let table_row = self.is_table_row.take().unwrap();
                            if !table_row.is_empty() {
                                self.is_table.as_mut().unwrap().push_row(table_row);
                            }
                        }
                        "table" => {
                            let is_table = self.is_table.take();
                            self.push_element(is_table.unwrap().into());
                            self.push_spacer();
                        }
                        "a" => self.is_link = None,
                        "code" => self.is_code = false,
                        "p" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.align = None;
                            self.text_align = None;
                        }
                        "em" | "i" => self.is_italic = false,
                        "strong" | "bold" => self.is_bold = false,
                        "h1" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = None;
                            self.align = None;
                            self.text_align = None;
                            self.is_underlined = false;
                        }
                        "h2" | "h3" | "h4" | "h5" | "h6" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_header = None;
                            self.align = None;
                            self.text_align = None;
                        }
                        "li" => {
                            self.push_current_textbox();
                            self.is_list_item = false
                        }
                        "ul" | "ol" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.global_indent -= DEFAULT_MARGIN / 4.;
                            self.current_textbox.indent = self.global_indent;
                            self.list_type = None;
                        }
                        "pre" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.is_pre_formated = false;
                            self.current_textbox.set_code_block(false);
                        }
                        _ => {}
                    },
                }
            }
            CharacterTokens(str) => {
                let str = str.to_string();
                if !(self.current_textbox.texts.is_empty() && str.trim().is_empty()) {
                    if str == "\n" {
                        if self.is_pre_formated {
                            /*
                            if let Some(ref mut last) = self.current_textbox.texts.last_mut() {
                                last.text.push('\n');
                            }
                            */
                            self.push_current_textbox();
                            self.current_textbox.is_code_block = true;
                        } else if !self.current_textbox.texts.is_empty() {
                            self.current_textbox.texts.push(Text::new(
                                " ".to_string(),
                                self.hidpi_scale,
                                self.theme.text_color,
                            ));
                        }
                    } else {
                        // check if str is whitespace only
                        let mut text = Text::new(str, self.hidpi_scale, self.theme.text_color);
                        if self.is_code {
                            text = text
                                .with_color(self.theme.code_color)
                                .with_font(1)
                                .with_size(18.)
                        }
                        if let Some(ref link) = self.is_link {
                            text = text.with_link(link.clone());
                            text = text.with_color(self.theme.link_color);
                        }
                        if let Some(Header(size)) = self.is_header {
                            text = text.with_size(size).make_bold(true);
                        }
                        if self.is_list_item {
                            if let Some(ListType::Ordered(ref mut index)) = self.list_type {
                                self.current_textbox.texts.push(
                                    Text::new(
                                        format!("{}. ", index),
                                        self.hidpi_scale,
                                        self.theme.text_color,
                                    )
                                    .make_bold(true),
                                );
                                *index += 1;
                            } else {
                                self.current_textbox.texts.push(
                                    Text::new(
                                        "· ".to_string(),
                                        self.hidpi_scale,
                                        self.theme.text_color,
                                    )
                                    .make_bold(true),
                                )
                            }
                            self.is_list_item = false;
                        }
                        if self.is_bold {
                            text = text.make_bold(true);
                        }
                        if self.is_italic {
                            text = text.make_italic(true);
                        }
                        if self.is_underlined {
                            text = text.make_underlined(true);
                        }
                        if self.is_striked {
                            text = text.make_striked(true);
                        }
                        if self.is_small {
                            text = text.with_size(12.);
                        }
                        if let Some(ref mut table_header) = self.is_table_header {
                            table_header.texts.push(text);
                        } else if let Some(ref mut table_data) = self.is_table_data {
                            table_data.texts.push(text);
                        } else {
                            self.current_textbox.texts.push(text);
                        }
                    }
                }
            }
            EOFToken => {
                self.push_element(self.current_textbox.clone().into());
                self.window.request_redraw();
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum ThemeOption {
    Dark,
    Light,
}

use clap::Parser;
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(value_parser)]
    file_path: PathBuf,

    #[clap(short, long, value_parser, default_value = "light")]
    theme: ThemeOption,

    #[clap(short, long, value_parser)]
    scale: Option<f32>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let theme = match args.theme {
        ThemeOption::Dark => color::DARK_DEFAULT,
        ThemeOption::Light => color::LIGHT_DEFAULT,
    };
    let md_string = std::fs::read_to_string(&args.file_path)
        .with_context(|| format!("No file found at {:?}", args.file_path))?;

    let inlyne = pollster::block_on(Inlyne::new(theme, args.scale.clone()));
    let theme = inlyne.renderer.theme.clone();
    let element_queue_clone = inlyne.element_queue.clone();
    let hidpi_scale = args.scale.unwrap_or(inlyne.window.scale_factor() as f32);
    let window_clone = inlyne.window.clone();
    std::thread::spawn(move || {
        let sink = TokenPrinter {
            current_textbox: TextBox::new(Vec::new(), hidpi_scale, &theme),
            is_link: None,
            is_header: None,
            is_code: false,
            is_list_item: false,
            is_bold: false,
            is_italic: false,
            is_underlined: false,
            is_striked: false,
            is_pre_formated: false,
            list_type: None,
            global_indent: 0.,
            align: None,
            text_align: None,
            is_table: None,
            is_table_data: None,
            is_table_header: None,
            is_table_row: None,
            is_small: false,
            hidpi_scale,
            theme,
            element_queue: element_queue_clone,
            window: window_clone,
        };
        let mut input = BufferQueue::new();
        let mut options = ComrakOptions::default();
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.parse.smart = true;
        options.render.unsafe_ = true;
        let htmlified = markdown_to_html(&md_string, &options);

        input.push_back(
            Tendril::from_str(&htmlified)
                .unwrap()
                .try_reinterpret::<fmt::UTF8>()
                .unwrap(),
        );

        let mut tok = Tokenizer::new(sink, TokenizerOpts::default());
        let _ = tok.feed(&mut input);
        assert!(input.is_empty());
        tok.end();
    });
    inlyne.run();

    Ok(())
}
