use crate::color::native_color;
use crate::image::Image;
use crate::image::ImageSize;
use crate::positioner::Positioned;
use crate::positioner::Row;
use crate::positioner::Section;
use crate::positioner::Spacer;
use crate::positioner::DEFAULT_MARGIN;
use crate::table::Table;
use crate::ImageCache;
use crate::InlyneEvent;

use crate::color::Theme;
use crate::text::{Text, TextBox};
use crate::utils::{markdown_to_html, Align};
use crate::Element;

use glyphon::FamilyOwned;
use html5ever::local_name;
use html5ever::tendril::*;
use html5ever::tokenizer::BufferQueue;
use html5ever::tokenizer::TagKind;
use html5ever::tokenizer::TagToken;
use html5ever::tokenizer::{Token, TokenSink, TokenSinkResult};
use html5ever::tokenizer::{Tokenizer, TokenizerOpts};
use html5ever::Attribute;
use wgpu::TextureFormat;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;
use Token::{CharacterTokens, EOFToken};

use std::collections::VecDeque;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

mod html {
    use crate::{positioner::Section, table::Table, text::TextBox, utils::Align};

    pub enum HeaderType {
        H1,
        H2,
        H3,
        H4,
        H5,
        H6,
    }

    impl HeaderType {
        pub fn text_size(&self) -> f32 {
            match &self {
                Self::H1 => 32.,
                Self::H2 => 24.,
                Self::H3 => 18.72,
                Self::H4 => 16.,
                Self::H5 => 13.28,
                Self::H6 => 10.72,
            }
        }
    }

    pub struct Header {
        pub header_type: HeaderType,
        pub align: Option<Align>,
    }

    #[derive(Debug)]
    pub enum ListType {
        Ordered(usize),
        Unordered,
    }

    pub struct List {
        pub list_type: ListType,
    }

    // Represents the number of parent text option tags the current element is a child of
    #[derive(Default)]
    pub struct TextOptions {
        pub underline: usize,
        pub bold: usize,
        pub italic: usize,
        pub strike_through: usize,
        pub small: usize,
        pub code: usize,
        pub pre_formatted: usize,
        pub block_quote: usize,
        pub link: Vec<String>,
    }

    pub enum Element {
        List(List),
        ListItem,
        Input,
        Table(Table),
        TableRow(Vec<TextBox>),
        Header(Header),
        Paragraph(Option<Align>),
        Div(Option<Align>),
        Details(Section),
        Summary,
    }
}

#[derive(Default, PartialEq, Eq)]
enum FontWeight {
    #[default]
    Normal,
    Bold,
}

impl FontWeight {
    fn new(s: &str) -> Self {
        match s {
            "bold" => Self::Bold,
            _ => Self::default(),
        }
    }
}

#[derive(Default, PartialEq, Eq)]
enum FontStyle {
    #[default]
    Normal,
    Italic,
}

impl FontStyle {
    fn new(s: &str) -> Self {
        match s {
            "italic" => Self::Italic,
            _ => Self::default(),
        }
    }
}

#[derive(Default)]
struct State {
    global_indent: f32,
    element_stack: Vec<html::Element>,
    text_options: html::TextOptions,
    span_color: [f32; 4],
    span_weight: FontWeight,
    span_style: FontStyle,
    // Stores the row and a counter of newlines after each image
    inline_images: Option<(Row, usize)>,
    pending_anchor: Option<String>,
}

pub struct HtmlInterpreter {
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    current_textbox: TextBox,
    hidpi_scale: f32,
    theme: Theme,
    surface_format: TextureFormat,
    window: Arc<Window>,
    state: State,
    file_path: PathBuf,
    // Whether the interpreters is allowed to queue elements
    pub should_queue: Arc<AtomicBool>,
    // Whether interpreter should stop queuing till next received file
    stopped: bool,
    first_pass: bool,
    image_cache: ImageCache,
    event_proxy: EventLoopProxy<InlyneEvent>,
}

impl HtmlInterpreter {
    pub fn new(
        window: Arc<Window>,
        element_queue: Arc<Mutex<VecDeque<Element>>>,
        theme: Theme,
        surface_format: TextureFormat,
        hidpi_scale: f32,
        file_path: PathBuf,
        image_cache: ImageCache,
        event_proxy: EventLoopProxy<InlyneEvent>,
    ) -> Self {
        Self {
            window,
            element_queue,
            current_textbox: TextBox::new(Vec::new(), hidpi_scale),
            hidpi_scale,
            surface_format,
            state: State {
                span_color: native_color(theme.code_color, &surface_format),
                ..Default::default()
            },
            theme,
            file_path,
            should_queue: Arc::new(AtomicBool::new(true)),
            stopped: false,
            first_pass: true,
            image_cache,
            event_proxy,
        }
    }

    pub fn interpret_md(self, receiver: mpsc::Receiver<String>) {
        let mut input = BufferQueue::new();

        let span_color = native_color(self.theme.code_color, &self.surface_format);
        let code_highlighter = self.theme.code_highlighter.clone();
        let mut tok = Tokenizer::new(self, TokenizerOpts::default());

        for md_string in receiver {
            if tok
                .sink
                .should_queue
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                tok.sink.state = State {
                    span_color,
                    ..Default::default()
                };
                tok.sink.current_textbox = TextBox::new(Vec::new(), tok.sink.hidpi_scale);
                tok.sink.stopped = false;
                let htmlified = markdown_to_html(&md_string, code_highlighter.clone());

                input.push_back(
                    Tendril::from_str(&htmlified)
                        .unwrap()
                        .try_reinterpret::<fmt::UTF8>()
                        .unwrap(),
                );

                let _ = tok.feed(&mut input);
                assert!(input.is_empty());
                tok.end();
            }
        }
    }

    // Searches the currently nested elements for align attribute
    fn find_current_align(&self) -> Option<Align> {
        for element in self.state.element_stack.iter().rev() {
            if let html::Element::Div(Some(elem_align))
            | html::Element::Paragraph(Some(elem_align))
            | html::Element::Header(html::Header {
                align: Some(elem_align),
                ..
            }) = element
            {
                return Some(*elem_align);
            }
        }
        None
    }

    fn push_current_textbox(&mut self) {
        // Push any inline images
        if let Some((row, count)) = self.state.inline_images.take() {
            if count == 0 {
                self.push_element(row);
                self.push_spacer();
            } else {
                self.state.inline_images = Some((row, count))
            }
        }

        if !self.current_textbox.texts.is_empty() {
            let mut empty = true;
            for text in &self.current_textbox.texts {
                if !text.text.trim().is_empty() {
                    empty = false;
                    break;
                }
            }
            if !empty {
                self.current_textbox.indent = self.state.global_indent;
                let section = self.state.element_stack.iter_mut().rev().find_map(|e| {
                    if let html::Element::Details(section) = e {
                        Some(section)
                    } else {
                        None
                    }
                });
                if let Some(section) = section {
                    section
                        .elements
                        .push(Positioned::new(self.current_textbox.clone()));
                } else {
                    self.push_element(self.current_textbox.clone());
                }
            }
        }
        self.current_textbox = TextBox::new(Vec::new(), self.hidpi_scale);
        self.current_textbox.indent = self.state.global_indent;
    }
    fn push_spacer(&mut self) {
        self.push_element(Spacer::new(5., false));
    }
    fn push_element<I: Into<Element>>(&mut self, element: I) {
        self.element_queue.lock().unwrap().push_back(element.into());
        if self.first_pass {
            self.window.request_redraw()
        }
    }
}

impl TokenSink for HtmlInterpreter {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        if !self.should_queue.load(std::sync::atomic::Ordering::Relaxed) {
            self.stopped = true;
        }
        if self.stopped {
            return TokenSinkResult::Continue;
        }
        match token {
            TagToken(tag) => {
                let tag_name = tag.name.to_string();
                match tag.kind {
                    TagKind::StartTag => match tag_name.as_str() {
                        "blockquote" => {
                            // FIXME blockquotes in list have no marker
                            self.push_current_textbox();
                            self.state.text_options.block_quote += 1;
                            self.state.global_indent += DEFAULT_MARGIN / 2.;
                            self.current_textbox
                                .set_quote_block(Some(self.state.text_options.block_quote));
                        }
                        "th" => self.state.text_options.bold += 1,
                        "td" => {}
                        "table" => {
                            self.push_spacer();
                            self.state
                                .element_stack
                                .push(html::Element::Table(Table::new()));
                        }
                        "a" => {
                            for Attribute { name, value } in tag.attrs {
                                if name.local == local_name!("href") {
                                    self.state.text_options.link.push(value.to_string());
                                }
                                if name.local == local_name!("id") {
                                    let anchor_name = format!("#{}", value);
                                    self.current_textbox.set_anchor(Some(anchor_name));
                                }
                            }
                        }
                        "small" => self.state.text_options.small += 1,
                        "br" => self.push_current_textbox(),
                        "ins" | "u" => self.state.text_options.underline += 1,
                        "del" | "s" => self.state.text_options.strike_through += 1,
                        "img" => {
                            let mut align = None;
                            let mut size = None;
                            for attr in &tag.attrs {
                                match attr.name.local {
                                    local_name!("align") => match attr.value.to_string().as_str() {
                                        "center" => align = Some(Align::Center),
                                        "left" => align = Some(Align::Left),
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
                            align = align.or_else(|| self.find_current_align());
                            for attr in tag.attrs {
                                if attr.name.local == local_name!("src") {
                                    let align = align.as_ref().unwrap_or(&Align::Left);
                                    let src = attr.value.to_string();
                                    let is_url =
                                        src.starts_with("http://") || src.starts_with("https://");
                                    let mut image = match self.image_cache.lock().unwrap().get(&src)
                                    {
                                        Some(image_data) if is_url => Image::from_image_data(
                                            image_data.clone(),
                                            self.hidpi_scale,
                                        )
                                        .with_align(*align),
                                        _ => Image::from_src(
                                            src.clone(),
                                            self.file_path.clone(),
                                            self.hidpi_scale,
                                            self.event_proxy.clone(),
                                        )
                                        .unwrap()
                                        .with_align(*align),
                                    };

                                    if let Some(link) = self.state.text_options.link.last() {
                                        image.set_link((*link).clone())
                                    }
                                    if let Some(size) = size {
                                        image = image.with_size(size);
                                    }

                                    if align == &Align::Left {
                                        if let Some((row, count)) = &mut self.state.inline_images {
                                            row.elements.push(Positioned::new(image));
                                            // Restart newline count
                                            *count = 1;
                                        } else {
                                            self.state.inline_images = Some((
                                                Row::new(
                                                    vec![Positioned::new(image)],
                                                    self.hidpi_scale,
                                                ),
                                                1,
                                            ));
                                        }
                                    } else {
                                        self.push_element(image);
                                        self.push_spacer();
                                    }
                                    break;
                                }
                            }
                        }
                        "div" | "p" => {
                            self.push_current_textbox();

                            // Push potentially pending anchor from containing li
                            let anchor_name = self.state.pending_anchor.take();
                            if anchor_name.is_some() {
                                self.current_textbox.set_anchor(anchor_name);
                            }

                            let mut align = None;
                            for attr in tag.attrs {
                                if attr.name.local == local_name!("align")
                                    || attr.name.local == *"text-align"
                                {
                                    match attr.value.to_string().as_str() {
                                        "left" => align = Some(Align::Left),
                                        "center" => align = Some(Align::Center),
                                        "right" => align = Some(Align::Right),
                                        _ => {}
                                    }
                                }
                            }
                            if let Some(align) = align.or_else(|| self.find_current_align()) {
                                self.current_textbox.set_align(align);
                            }
                            self.state.element_stack.push(match tag_name.as_str() {
                                "div" => html::Element::Div(align),
                                "p" => html::Element::Paragraph(align),
                                _ => unreachable!("Arm matches on div and p"),
                            });
                        }
                        "em" | "i" => self.state.text_options.italic += 1,
                        "bold" | "strong" => self.state.text_options.bold += 1,
                        "code" => self.state.text_options.code += 1,
                        "li" => {
                            self.state.element_stack.push(html::Element::ListItem);
                            for Attribute { name, value } in tag.attrs {
                                if name.local == local_name!("id") {
                                    let anchor_name = format!("#{}", value);
                                    self.state.pending_anchor = Some(anchor_name);
                                }
                            }
                        }
                        "ul" => {
                            self.push_current_textbox();
                            self.state.global_indent += DEFAULT_MARGIN / 2.;
                            self.state
                                .element_stack
                                .push(html::Element::List(html::List {
                                    list_type: html::ListType::Unordered,
                                }));
                        }
                        "ol" => {
                            let mut start_index = 1;
                            for attr in tag.attrs {
                                if attr.name.local == local_name!("start") {
                                    start_index = attr.value.parse::<usize>().unwrap();
                                    break;
                                }
                            }
                            self.push_current_textbox();
                            self.state.global_indent += DEFAULT_MARGIN / 2.;
                            self.state
                                .element_stack
                                .push(html::Element::List(html::List {
                                    list_type: html::ListType::Ordered(start_index),
                                }));
                        }
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            let mut align = None;
                            for attr in tag.attrs {
                                if attr.name.local == local_name!("align")
                                    || attr.name.local == *"text-align"
                                {
                                    match attr.value.to_string().as_str() {
                                        "left" => align = Some(Align::Left),
                                        "center" => align = Some(Align::Center),
                                        "right" => align = Some(Align::Right),
                                        _ => {}
                                    }
                                }
                            }
                            let header_type = match tag_name.as_str() {
                                "h1" => html::HeaderType::H1,
                                "h2" => html::HeaderType::H2,
                                "h3" => html::HeaderType::H3,
                                "h4" => html::HeaderType::H4,
                                "h5" => html::HeaderType::H5,
                                "h6" => html::HeaderType::H6,
                                _ => unreachable!(),
                            };
                            self.push_current_textbox();
                            self.push_spacer();
                            if let html::HeaderType::H1 = header_type {
                                self.state.text_options.underline += 1;
                            }
                            self.state
                                .element_stack
                                .push(html::Element::Header(html::Header { header_type, align }));
                            self.current_textbox.set_align(align.unwrap_or(Align::Left));
                        }
                        "pre" => {
                            self.push_current_textbox();
                            for Attribute { name, value } in &tag.attrs {
                                if &name.local == "style" {
                                    let style = value.to_string();
                                    if let Some(hex_str) = style
                                        .split(';')
                                        .find_map(|style| style.strip_prefix("background-color:#"))
                                    {
                                        if let Ok(hex) = u32::from_str_radix(hex_str, 16) {
                                            let bg_color = native_color(hex, &self.surface_format);
                                            self.current_textbox
                                                .set_background_color(Some(bg_color));
                                        }
                                    }
                                }
                            }
                            self.state.text_options.pre_formatted += 1;
                            self.current_textbox.set_code_block(true);
                        }
                        "tr" => {
                            self.state
                                .element_stack
                                .push(html::Element::TableRow(Vec::new()));
                        }
                        // HACK: spans are only supported enough to get syntax highlighting in code
                        // blocks working
                        "span" => {
                            for Attribute { name, value } in &tag.attrs {
                                if &name.local == "style" {
                                    let styles = value.to_string();
                                    for style in styles.split(';') {
                                        if let Some(hex_str) = style.strip_prefix("color:#") {
                                            if let Ok(hex) = u32::from_str_radix(hex_str, 16) {
                                                self.state.span_color =
                                                    native_color(hex, &self.surface_format);
                                            }
                                        } else if let Some(font_weight) =
                                            style.strip_prefix("font-weight:")
                                        {
                                            self.state.span_weight = FontWeight::new(font_weight);
                                        } else if let Some(font_style) =
                                            style.strip_prefix("font-style:")
                                        {
                                            self.state.span_style = FontStyle::new(font_style);
                                        }
                                    }
                                }
                            }
                        }
                        "input" => {
                            for Attribute { name, value } in &tag.attrs {
                                if &name.local == "type" {
                                    let value = value.to_string();
                                    if value == "checkbox" {
                                        self.push_current_textbox();
                                        self.current_textbox.set_checkbox(Some(
                                            tag.attrs
                                                .iter()
                                                .any(|attr| &attr.name.local == "checked"),
                                        ));
                                        self.state.element_stack.push(html::Element::Input);
                                    }
                                }
                            }
                        }
                        "details" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            let section = Section::new(None, vec![], self.hidpi_scale);
                            *section.hidden.borrow_mut() = true;
                            self.state
                                .element_stack
                                .push(html::Element::Details(section));
                        }
                        "summary" => {
                            self.push_current_textbox();
                            self.state.element_stack.push(html::Element::Summary);
                        }
                        "hr" => {
                            self.push_element(Spacer::new(5., true));
                        }
                        _ => {}
                    },
                    TagKind::EndTag => match tag_name.as_str() {
                        "ins" | "u" => self.state.text_options.underline -= 1,
                        "del" | "s" => self.state.text_options.strike_through -= 1,
                        "small" => self.state.text_options.small -= 1,
                        "th" => {
                            let mut table = None;
                            for element in self.state.element_stack.iter_mut().rev() {
                                if let html::Element::Table(ref mut html_table) = element {
                                    table = Some(html_table);
                                    break;
                                }
                            }
                            table.unwrap().push_header(self.current_textbox.clone());
                            self.current_textbox.texts.clear();
                            self.state.text_options.bold -= 1;
                        }
                        "td" => {
                            let table_row = self.state.element_stack.last_mut();
                            if let Some(html::Element::TableRow(ref mut row)) = table_row {
                                row.push(self.current_textbox.clone());
                            }
                            self.current_textbox.texts.clear();
                        }
                        "tr" => {
                            let table_row = self.state.element_stack.pop();
                            for element in self.state.element_stack.iter_mut().rev() {
                                if let html::Element::Table(ref mut table) = element {
                                    if let Some(html::Element::TableRow(row)) = table_row {
                                        if !row.is_empty() {
                                            table.push_row(row);
                                        }
                                        break;
                                    }
                                }
                            }
                            self.current_textbox.texts.clear();
                        }
                        "table" => {
                            if let Some(html::Element::Table(table)) =
                                self.state.element_stack.pop()
                            {
                                self.push_element(table);
                                self.push_spacer();
                            }
                        }
                        "a" => {
                            self.state.text_options.link.pop();
                        }
                        "code" => self.state.text_options.code -= 1,
                        "div" | "p" => {
                            self.push_current_textbox();
                            if tag_name == "p" {
                                self.push_spacer();
                            }
                            self.state.element_stack.pop();
                        }
                        "em" | "i" => self.state.text_options.italic -= 1,
                        "bold" | "strong" => self.state.text_options.bold -= 1,
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                            if tag_name.as_str() == "h1" {
                                self.state.text_options.underline -= 1;
                            }
                            let mut anchor_name = "#".to_string();
                            for text in &self.current_textbox.texts {
                                for char in text.text.chars() {
                                    if char.is_whitespace() || char == '-' {
                                        anchor_name.push('-');
                                    } else if char.is_alphanumeric() {
                                        anchor_name.push(char.to_ascii_lowercase());
                                    }
                                }
                            }
                            self.current_textbox.set_anchor(Some(anchor_name));
                            self.push_current_textbox();
                            self.push_spacer();
                            self.state.element_stack.pop();
                        }
                        "li" => {
                            // Pop pending anchor if nothing consumed it
                            let _ = self.state.pending_anchor.take();

                            self.push_current_textbox();
                            self.state.element_stack.pop();
                        }
                        "input" => {
                            self.push_current_textbox();
                            self.state.element_stack.pop();
                        }
                        "ul" | "ol" => {
                            self.push_current_textbox();
                            self.state.global_indent -= DEFAULT_MARGIN / 2.;
                            self.state.element_stack.pop();
                            if self.state.global_indent == 0. {
                                self.push_spacer();
                            }
                        }
                        "pre" => {
                            self.push_current_textbox();
                            self.push_spacer();
                            self.state.text_options.pre_formatted -= 1;
                            self.current_textbox.set_code_block(false);
                        }
                        "blockquote" => {
                            self.push_current_textbox();
                            self.state.text_options.block_quote -= 1;
                            self.state.global_indent -= DEFAULT_MARGIN / 2.;
                            self.current_textbox.set_quote_block(None);
                            if self.state.global_indent == 0. {
                                self.push_spacer();
                            }
                        }
                        "span" => {
                            self.state.span_color =
                                native_color(self.theme.code_color, &self.surface_format);
                            self.state.span_weight = FontWeight::default();
                            self.state.span_style = FontStyle::default();
                        }
                        "details" => {
                            self.push_current_textbox();
                            if let Some(html::Element::Details(section)) =
                                self.state.element_stack.pop()
                            {
                                self.push_element(section);
                            }
                            self.push_spacer();
                        }
                        "summary" => {
                            for element in self.state.element_stack.iter_mut().rev() {
                                if let html::Element::Details(ref mut section) = element {
                                    *section.summary =
                                        Some(Positioned::new(self.current_textbox.clone()));
                                    self.current_textbox.texts.clear();
                                    break;
                                }
                            }
                            self.state.element_stack.pop();
                        }
                        _ => {}
                    },
                }
            }
            CharacterTokens(str) => {
                let mut str = str.to_string();
                if str == "\n" {
                    if self.state.text_options.pre_formatted >= 1 {
                        self.current_textbox.texts.push(Text::new(
                            "\n".to_string(),
                            self.hidpi_scale,
                            native_color(self.theme.text_color, &self.surface_format),
                        ));
                    }
                    if let Some(last_text) = self.current_textbox.texts.last() {
                        if let Some(last_char) = last_text.text.chars().last() {
                            if !last_char.is_whitespace() {
                                self.current_textbox.texts.push(Text::new(
                                    " ".to_string(),
                                    self.hidpi_scale,
                                    native_color(self.theme.text_color, &self.surface_format),
                                ));
                            }
                        }
                    }
                    if let Some((row, newline_counter)) = self.state.inline_images.take() {
                        if newline_counter == 0 {
                            self.push_element(row);
                            self.push_spacer();
                        } else {
                            self.state.inline_images = Some((row, newline_counter - 1));
                        }
                    }
                } else if str.trim().is_empty() && self.state.text_options.pre_formatted == 0 {
                    if let Some(last_text) = self.current_textbox.texts.last() {
                        if let Some(last_char) = last_text.text.chars().last() {
                            if !last_char.is_whitespace() {
                                self.current_textbox.texts.push(Text::new(
                                    " ".to_string(),
                                    self.hidpi_scale,
                                    native_color(self.theme.text_color, &self.surface_format),
                                ));
                            }
                        }
                    }
                } else {
                    if self.current_textbox.texts.is_empty()
                        && self.state.text_options.pre_formatted == 0
                    {
                        str = str.trim_start().to_owned();
                    }

                    let mut text = Text::new(
                        str,
                        self.hidpi_scale,
                        native_color(self.theme.text_color, &self.surface_format),
                    );
                    if let Some(html::Element::ListItem) = self.state.element_stack.last() {
                        let mut list = None;
                        for element in self.state.element_stack.iter_mut().rev() {
                            if let html::Element::List(html_list) = element {
                                list = Some(html_list);
                            }
                        }
                        let list = list.expect("List ended unexpectedly");

                        if self.current_textbox.texts.is_empty() {
                            if let html::List {
                                list_type: html::ListType::Ordered(index),
                                ..
                            } = list
                            {
                                self.current_textbox.texts.push(
                                    Text::new(
                                        format!("{}. ", index),
                                        self.hidpi_scale,
                                        native_color(self.theme.text_color, &self.surface_format),
                                    )
                                    .make_bold(true),
                                );
                                *index += 1;
                            } else if let html::List {
                                list_type: html::ListType::Unordered,
                                ..
                            } = list
                            {
                                self.current_textbox.texts.push(
                                    Text::new(
                                        "· ".to_string(),
                                        self.hidpi_scale,
                                        native_color(self.theme.text_color, &self.surface_format),
                                    )
                                    .make_bold(true),
                                )
                            }
                        }
                    }
                    if self.state.text_options.block_quote >= 1 {
                        self.current_textbox
                            .set_quote_block(Some(self.state.text_options.block_quote));
                    }
                    if self.state.text_options.code >= 1 {
                        text = text
                            .with_color(self.state.span_color)
                            .with_family(FamilyOwned::Monospace);
                        if self.state.span_weight == FontWeight::Bold {
                            text = text.make_bold(true);
                        }
                        if self.state.span_style == FontStyle::Italic {
                            text = text.make_italic(true);
                        }
                        //.with_size(18.)
                    }
                    for elem in self.state.element_stack.iter().rev() {
                        if let html::Element::Header(header) = elem {
                            self.current_textbox.font_size = header.header_type.text_size();
                            text = text.make_bold(true);
                            break;
                        }
                    }
                    if let Some(link) = self.state.text_options.link.last() {
                        text = text.with_link((*link).clone());
                        text = text
                            .with_color(native_color(self.theme.link_color, &self.surface_format));
                    }
                    if self.state.text_options.bold >= 1 {
                        text = text.make_bold(true);
                    }
                    if self.state.text_options.italic >= 1 {
                        text = text.make_italic(true);
                    }
                    if self.state.text_options.underline >= 1 {
                        text = text.make_underlined(true);
                    }
                    if self.state.text_options.strike_through >= 1 {
                        text = text.make_striked(true);
                    }
                    if self.state.text_options.small >= 1 {
                        self.current_textbox.font_size = 12.;
                        //text = text.with_size(12.);
                        //FIXME
                    }
                    self.current_textbox.texts.push(text);
                }
            }
            EOFToken => {
                self.push_current_textbox();
                self.should_queue
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                self.first_pass = false;
                self.event_proxy
                    .send_event(InlyneEvent::PositionQueue)
                    .unwrap();
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
