pub mod color;
pub mod image;
pub mod renderer;
pub mod text;

use crate::image::Image;
use crate::image::ImageSize;
use crate::renderer::Rect;
use color::Theme;
use renderer::{Align, Renderer, Spacer};

use comrak::{markdown_to_html, ComrakOptions};
use html5ever::local_name;
use html5ever::tendril::*;
use html5ever::tokenizer::TagToken;
use html5ever::tokenizer::{BufferQueue, TagKind};
use html5ever::tokenizer::{Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts};
use text::{Text, TextBox};
use winit::event::{ElementState, MouseButton};
use winit::event_loop::EventLoopProxy;
use winit::{
    event::{Event, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{CursorIcon, Window},
};
use Token::{CharacterTokens, EOFToken};

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

use crate::renderer::DEFAULT_MARGIN;

#[derive(Debug)]
pub enum InlyneEvent {
    Reposition,
    Redraw,
}

pub enum Element {
    TextBox(TextBox),
    Spacer(Spacer),
    Image(Image),
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

pub struct Inlyne {
    window: Window,
    event_loop: EventLoop<InlyneEvent>,
    renderer: Renderer,
    element_queue: Arc<Mutex<VecDeque<Element>>>,
}

impl Inlyne {
    pub async fn new(theme: Theme) -> Self {
        let event_loop = EventLoop::<InlyneEvent>::with_user_event();
        let window = Window::new(&event_loop).unwrap();
        window.set_title("Inlyne");
        let renderer = Renderer::new(&window, event_loop.create_proxy(), theme).await;

        Self {
            window,
            event_loop,
            renderer,
            element_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn push<T: Into<Element>>(&mut self, element: T) {
        let element = element.into();
        self.renderer.push(element);
    }

    pub fn run(mut self) {
        let mut click_scheduled = false;
        let mut scrollbar_held = false;
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::Reposition => {
                        self.renderer.reposition();
                        self.renderer.redraw()
                    }
                    InlyneEvent::Redraw => {
                        for element in self.element_queue.lock().unwrap().drain(0..) {
                            self.renderer.push(element)
                        }
                        self.renderer.redraw();
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
                Event::RedrawRequested(_) => self.renderer.redraw(),
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
                        _ => unimplemented!(),
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        let mut over_text = false;
                        let screen_size = self.renderer.screen_size();
                        for element in self.renderer.elements.iter() {
                            let loc = (
                                position.x as f32,
                                position.y as f32 + self.renderer.scroll_y,
                            );
                            if element.contains(loc) {
                                if let Element::TextBox(ref text_box) = element.deref() {
                                    let bounds = element.bounds.as_ref().unwrap();
                                    let cursor = text_box.hovering_over(
                                        &mut self.renderer.glyph_brush,
                                        loc,
                                        bounds.pos,
                                        (
                                            screen_size.0 - bounds.pos.0 - renderer::DEFAULT_MARGIN,
                                            screen_size.1,
                                        ),
                                        self.renderer.hidpi_scale,
                                    );
                                    self.window.set_cursor_icon(cursor);
                                    over_text = true;
                                    if click_scheduled {
                                        text_box.click(
                                            &mut self.renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            self.renderer.hidpi_scale,
                                        );
                                        click_scheduled = false;
                                    }
                                    break;
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
                            click_scheduled = false;
                            if !scrollbar_held {
                                scrollbar_held = true;
                            }
                        }

                        if !over_text {
                            self.window.set_cursor_icon(CursorIcon::Default);
                        }
                    }
                    WindowEvent::MouseInput {
                        state,
                        button: MouseButton::Left,
                        ..
                    } => match state {
                        ElementState::Pressed => {
                            click_scheduled = true;
                        }
                        ElementState::Released => {
                            scrollbar_held = false;
                        }
                    },
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
    is_pre_formated: bool,
    global_indent: f32,
    align: Option<Align>,
    text_align: Option<Align>,
    theme: Theme,
    eventloop_proxy: EventLoopProxy<InlyneEvent>,
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
            self.current_textbox = TextBox::new(Vec::new());
        }
    }
    fn push_spacer(&mut self) {
        self.push_element(Spacer::new(10.).into());
    }
    fn push_element(&mut self, element: Element) {
        let mut element_queue = self.element_queue.lock().unwrap();
        element_queue.push_back(element);
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
                        "a" => {
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("href") {
                                    self.is_link = Some(attr.value.to_string());
                                    break;
                                }
                            }
                        }
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
                                    let mut image = Image::from_url(attr.value.to_string())
                                        .with_align(local_align.unwrap_or_else(|| align.clone()));
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
                            self.current_textbox.set_align(
                                self.text_align
                                    .as_ref()
                                    .unwrap_or_else(|| self.align.as_ref().unwrap_or(&Align::Left))
                                    .clone(),
                            );
                        }
                        "strong" => self.is_bold = true,
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
                        _ => {}
                    },
                    TagKind::EndTag => match tag_name.as_str() {
                        "a" => self.is_link = None,
                        "code" => self.is_code = false,
                        "p" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.align = None;
                            self.text_align = None;
                        }
                        "strong" => self.is_bold = false,
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
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
                            self.current_textbox.texts.push(Text::new(" ".to_string()));
                        }
                    } else {
                        // check if str is whitespace only
                        let mut text = Text::new(str);
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
                                self.current_textbox
                                    .texts
                                    .push(Text::new(format!("{}. ", index)).make_bold(true));
                                *index += 1;
                            } else {
                                self.current_textbox
                                    .texts
                                    .push(Text::new("· ".to_string()).make_bold(true))
                            }
                            self.is_list_item = false;
                        }
                        if self.is_bold {
                            text = text.make_bold(true);
                        }
                        self.current_textbox.texts.push(text);
                    }
                }
            }
            EOFToken => {
                self.push_element(self.current_textbox.clone().into());
                self.eventloop_proxy
                    .send_event(InlyneEvent::Redraw)
                    .unwrap();
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
}

fn main() {
    let args = Args::parse();
    let theme = match args.theme {
        ThemeOption::Dark => color::DARK_DEFAULT,
        ThemeOption::Light => color::LIGHT_DEFAULT,
    };
    let inlyne = pollster::block_on(Inlyne::new(theme));

    let mut md_file = File::open(args.file_path.as_path()).unwrap();
    let md_file_size = std::fs::metadata(args.file_path.as_path()).unwrap().len();
    let mut md_string = String::with_capacity(md_file_size as usize);
    md_file.read_to_string(&mut md_string).unwrap();

    let eventloop_proxy = inlyne.event_loop.create_proxy();
    let theme = inlyne.renderer.theme.clone();
    let element_queue_clone = inlyne.element_queue.clone();
    std::thread::spawn(move || {
        let sink = TokenPrinter {
            current_textbox: TextBox::new(Vec::new()),
            is_link: None,
            is_header: None,
            is_code: false,
            is_list_item: false,
            is_bold: false,
            is_pre_formated: false,
            list_type: None,
            global_indent: 0.,
            align: None,
            text_align: None,
            theme,
            eventloop_proxy,
            element_queue: element_queue_clone,
        };
        let mut input = BufferQueue::new();
        let mut options = ComrakOptions::default();
        options.extension.table = true;
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
}
