pub mod color;
pub mod image;
pub mod renderer;
pub mod text;

use crate::image::Image;
use crate::image::ImageSize;
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
use winit::{
    event::{Event, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{CursorIcon, Window},
};
use Token::{CharacterTokens, EOFToken};

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
}

#[derive(Debug)]
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
    renderer: Arc<Mutex<Renderer>>,
}

impl Inlyne {
    pub async fn new(theme: Theme) -> Self {
        let event_loop = EventLoop::<InlyneEvent>::with_user_event();
        let window = Window::new(&event_loop).unwrap();
        window.set_title("Inlyne");
        let renderer = Arc::new(Mutex::new(
            Renderer::new(&window, event_loop.create_proxy(), theme).await,
        ));

        Self {
            window,
            event_loop,
            renderer,
        }
    }

    pub fn push<T: Into<Element>>(&mut self, element: T) {
        let element = element.into();
        self.renderer.lock().unwrap().push(element);
    }

    pub fn run(self) {
        let mut click_scheduled = false;
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::UserEvent(inlyne_event) => match inlyne_event {
                    InlyneEvent::Reposition => {
                        self.renderer.lock().unwrap().reposition();
                        self.window.request_redraw();
                    }
                },
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    let mut renderer = &mut *(self.renderer.lock().unwrap());
                    renderer.config.width = size.width;
                    renderer.config.height = size.height;
                    renderer
                        .surface
                        .configure(&renderer.device, &renderer.config);
                    renderer.reposition();
                    self.window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    let renderer = &mut *(self.renderer.lock().unwrap());
                    renderer.redraw(renderer.reserved_height)
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::MouseWheel { delta, .. } => match delta {
                        MouseScrollDelta::PixelDelta(pos) => {
                            {
                                let mut renderer = &mut *(self.renderer.lock().unwrap());
                                let screen_height = renderer.screen_height();
                                if renderer.reserved_height > screen_height {
                                    renderer.scroll_y -= pos.y as f32;

                                    if renderer.scroll_y.is_sign_negative() {
                                        renderer.scroll_y = 0.;
                                    } else if renderer.scroll_y
                                        >= (renderer.reserved_height - screen_height)
                                    {
                                        renderer.scroll_y =
                                            renderer.reserved_height - screen_height;
                                    }
                                }
                            }
                            self.window.request_redraw();
                        }
                        _ => unimplemented!(),
                    },
                    WindowEvent::CursorMoved { position, .. } => {
                        let renderer = &mut *(self.renderer.lock().unwrap());
                        let mut over_text = false;
                        for element in &renderer.elements {
                            let loc = (position.x as f32, position.y as f32 + renderer.scroll_y);
                            if element.contains(loc) {
                                if let Element::TextBox(ref text_box) = element.deref() {
                                    let screen_size = renderer.screen_size();
                                    let bounds = element.bounds.as_ref().unwrap();
                                    let cursor = text_box.hovering_over(
                                        &mut renderer.glyph_brush,
                                        loc,
                                        bounds.pos,
                                        (
                                            screen_size.0 - bounds.pos.0 - renderer::DEFAULT_MARGIN,
                                            screen_size.1,
                                        ),
                                        renderer.hidpi_scale,
                                    );
                                    self.window.set_cursor_icon(cursor);
                                    over_text = true;
                                    if click_scheduled {
                                        text_box.click(
                                            &mut renderer.glyph_brush,
                                            loc,
                                            bounds.pos,
                                            (
                                                screen_size.0
                                                    - bounds.pos.0
                                                    - renderer::DEFAULT_MARGIN,
                                                screen_size.1,
                                            ),
                                            renderer.hidpi_scale,
                                        );
                                        click_scheduled = false;
                                    }
                                    break;
                                }
                            }
                        }
                        if !over_text {
                            self.window.set_cursor_icon(CursorIcon::Default);
                        }
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        click_scheduled = true;
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
    renderer: Arc<Mutex<Renderer>>,
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
                self.renderer
                    .lock()
                    .unwrap()
                    .push(self.current_textbox.clone().into());
            }
            self.current_textbox = TextBox::new(Vec::new());
        }
    }
    fn push_spacer(&mut self) {
        self.renderer.lock().unwrap().push(Spacer::new(20.).into());
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
                                    self.renderer.lock().unwrap().push(image.into());
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
                                .with_color(self.renderer.lock().unwrap().theme.code_color)
                                .with_font(1)
                        }
                        if let Some(ref link) = self.is_link {
                            text = text.with_link(link.clone());
                            text = text.with_color(self.renderer.lock().unwrap().theme.link_color);
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
                self.renderer
                    .lock()
                    .unwrap()
                    .push(self.current_textbox.clone().into());
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

    let renderer_clone = inlyne.renderer.clone();
    std::thread::spawn(move || {
        let sink = TokenPrinter {
            renderer: renderer_clone,
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
        };
        let mut input = BufferQueue::new();
        let mut options = ComrakOptions::default();
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
