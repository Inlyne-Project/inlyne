mod html;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};

use crate::color::{native_color, Theme};
use crate::image::{Image, ImageData, ImageSize};
use crate::positioner::{Positioned, Row, Section, Spacer, DEFAULT_MARGIN};
use crate::table::Table;
use crate::text::{Text, TextBox};
use crate::utils::{markdown_to_html, Align};
use crate::{Element, ImageCache, InlyneEvent};
use html::{Attr, AttrIter, FontStyle, FontWeight, Style, StyleIter, TextDecoration};

use comrak::Anchorizer;
use glyphon::FamilyOwned;
use html5ever::tendril::*;
use html5ever::tokenizer::{
    BufferQueue, TagKind, TagToken, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use wgpu::TextureFormat;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;
use Token::{CharacterTokens, EOFToken};

#[derive(Default)]
struct State {
    global_indent: f32,
    element_stack: Vec<html::Element>,
    text_options: html::TextOptions,
    span_color: [f32; 4],
    span_bg: [f32; 4],
    span_weight: FontWeight,
    span_style: FontStyle,
    span_decor: TextDecoration,
    // Stores the row and a counter of newlines after each image
    inline_images: Option<(Row, usize)>,
    pending_anchor: Option<String>,
    pending_list_prefix: Option<String>,
    anchorizer: Anchorizer,
}

// Images are loaded in a separate thread and use a callback to indicate when they're finished
pub trait ImageCallback {
    fn loaded_image(&self, src: String, image_data: Arc<Mutex<Option<ImageData>>>);
}

// External state from the interpreter that we want to stub out for testing
trait WindowInteractor {
    fn finished_single_doc(&self);
    fn request_redraw(&self);
    fn image_callback(&self) -> Box<dyn ImageCallback + Send>;
}

struct EventLoopCallback(EventLoopProxy<InlyneEvent>);

impl ImageCallback for EventLoopCallback {
    fn loaded_image(&self, src: String, image_data: Arc<Mutex<Option<ImageData>>>) {
        let event = InlyneEvent::LoadedImage(src, image_data);
        self.0.send_event(event).unwrap();
    }
}

// A real interactive window that is being used with `HtmlInterpreter`
struct LiveWindow {
    window: Arc<Window>,
    event_proxy: EventLoopProxy<InlyneEvent>,
}

impl WindowInteractor for LiveWindow {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn image_callback(&self) -> Box<dyn ImageCallback + Send> {
        Box::new(EventLoopCallback(self.event_proxy.clone()))
    }

    fn finished_single_doc(&self) {
        self.event_proxy
            .send_event(InlyneEvent::PositionQueue)
            .unwrap();
    }
}

pub struct HtmlInterpreter {
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    current_textbox: TextBox,
    hidpi_scale: f32,
    theme: Theme,
    surface_format: TextureFormat,
    state: State,
    file_path: PathBuf,
    // Whether the interpreters is allowed to queue elements
    pub should_queue: Arc<AtomicBool>,
    // Whether interpreter should stop queuing till next received file
    stopped: bool,
    first_pass: bool,
    image_cache: ImageCache,
    window: Box<dyn WindowInteractor + Send>,
}

impl HtmlInterpreter {
    // FIXME: clippy is probably right here, but I didn't want to hold up setting up clippy for the
    // rest of the repo just because of here
    #[allow(clippy::too_many_arguments)]
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
        let live_window = LiveWindow {
            window,
            event_proxy,
        };
        Self::new_with_interactor(
            element_queue,
            theme,
            surface_format,
            hidpi_scale,
            file_path,
            image_cache,
            Box::new(live_window),
        )
    }

    fn new_with_interactor(
        element_queue: Arc<Mutex<VecDeque<Element>>>,
        theme: Theme,
        surface_format: TextureFormat,
        hidpi_scale: f32,
        file_path: PathBuf,
        image_cache: ImageCache,
        window: Box<dyn WindowInteractor + Send>,
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
        }
    }

    pub fn interpret_md(self, receiver: mpsc::Receiver<String>) {
        let mut input = BufferQueue::new();

        let span_color = native_color(self.theme.code_color, &self.surface_format);
        let code_highlighter = self.theme.code_highlighter.clone();
        let mut tok = Tokenizer::new(self, TokenizerOpts::default());

        for md_string in receiver {
            tracing::debug!(
                "Received markdown for interpretation: {} bytes",
                md_string.len()
            );

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
                        "th" => {
                            self.state.text_options.bold += 1;
                            let align = html::find_align(&tag.attrs);
                            self.current_textbox.set_align(align.unwrap_or(Align::Left));
                        }
                        "td" => {
                            let align = html::find_align(&tag.attrs);
                            self.current_textbox.set_align(align.unwrap_or(Align::Left));
                        }
                        "table" => {
                            self.push_spacer();
                            self.state
                                .element_stack
                                .push(html::Element::Table(Table::new()));
                        }
                        "a" => {
                            for attr in AttrIter::new(&tag.attrs) {
                                match attr {
                                    Attr::Href(link) => self.state.text_options.link.push(link),
                                    Attr::Anchor(a) => self.current_textbox.set_anchor(Some(a)),
                                    _ => {}
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
                            let mut src = None;
                            for attr in AttrIter::new(&tag.attrs) {
                                match attr {
                                    Attr::Align(a) => align = Some(a),
                                    Attr::Width(w) => size = Some(ImageSize::PxWidth(w)),
                                    Attr::Height(h) => size = Some(ImageSize::PxHeight(h)),
                                    Attr::Src(s) => src = Some(s),
                                    _ => {}
                                }
                            }
                            align = align.or_else(|| self.find_current_align());
                            if let Some(src) = src {
                                let align = align.as_ref().unwrap_or(&Align::Left);
                                let is_url =
                                    src.starts_with("http://") || src.starts_with("https://");
                                let mut image = match self.image_cache.lock().unwrap().get(&src) {
                                    Some(image_data) if is_url => {
                                        Image::from_image_data(image_data.clone(), self.hidpi_scale)
                                            .with_align(*align)
                                    }
                                    _ => Image::from_src(
                                        src.clone(),
                                        self.file_path.clone(),
                                        self.hidpi_scale,
                                        self.window.image_callback(),
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
                            }
                        }
                        "div" | "p" => {
                            self.push_current_textbox();

                            // Push potentially pending anchor from containing li
                            let anchor_name = self.state.pending_anchor.take();
                            if let Some(anchor) = anchor_name {
                                let anchorized = self.state.anchorizer.anchorize(anchor);
                                self.current_textbox
                                    .set_anchor(Some(format!("#{anchorized}")));
                            }

                            let align = html::find_align(&tag.attrs);
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
                            for attr in AttrIter::new(&tag.attrs) {
                                if let Attr::Anchor(anchor) = attr {
                                    self.state.pending_anchor = Some(anchor);
                                }
                            }

                            // Push a pending list prefix based on the list type
                            let mut list = None;
                            for element in self.state.element_stack.iter_mut().rev() {
                                if let html::Element::List(html_list) = element {
                                    list = Some(html_list);
                                    break;
                                }
                            }
                            let list = list.expect("List ended unexpectedly");
                            if self.current_textbox.texts.is_empty() {
                                let prefix = match &mut list.list_type {
                                    html::ListType::Ordered(index) => {
                                        *index += 1;
                                        format!("{}. ", *index - 1)
                                    }
                                    html::ListType::Unordered => "· ".to_owned(),
                                };

                                self.state.pending_list_prefix = Some(prefix);
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
                            for attr in AttrIter::new(&tag.attrs) {
                                if let Attr::Start(start) = attr {
                                    start_index = start;
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
                            let align = html::find_align(&tag.attrs);
                            let header_type = html::HeaderType::new(&tag_name).unwrap();
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
                            let style_str = html::find_style(&tag.attrs).unwrap_or_default();
                            for style in StyleIter::new(&style_str) {
                                if let Style::BackgroundColor(color) = style {
                                    let native_color = native_color(color, &self.surface_format);
                                    self.current_textbox
                                        .set_background_color(Some(native_color));
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
                            let style_str = html::find_style(&tag.attrs).unwrap_or_default();
                            for style in StyleIter::new(&style_str) {
                                match style {
                                    Style::Color(color) => {
                                        self.state.span_color =
                                            native_color(color, &self.surface_format)
                                    }
                                    Style::BackgroundColor(color) => {
                                        self.state.span_bg =
                                            native_color(color, &self.surface_format)
                                    }
                                    Style::FontWeight(weight) => self.state.span_weight = weight,
                                    Style::FontStyle(style) => self.state.span_style = style,
                                    Style::TextDecoration(decor) => self.state.span_decor = decor,
                                }
                            }
                        }
                        "input" => {
                            let mut is_checkbox = false;
                            let mut is_checked = false;
                            for attr in AttrIter::new(&tag.attrs) {
                                match attr {
                                    Attr::IsCheckbox => is_checkbox = true,
                                    Attr::IsChecked => is_checked = true,
                                    _ => {}
                                }
                            }
                            if is_checkbox {
                                // Checkbox uses a custom prefix, so remove pending text prefix
                                let _ = self.state.pending_list_prefix.take();
                                self.current_textbox.set_checkbox(Some(is_checked));
                                self.state.element_stack.push(html::Element::Input);
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
                            let anchor_name = self
                                .current_textbox
                                .texts
                                .iter()
                                .flat_map(|t| t.text.chars())
                                .collect();
                            let anchorized = self.state.anchorizer.anchorize(anchor_name);
                            self.current_textbox
                                .set_anchor(Some(format!("#{anchorized}")));
                            self.push_current_textbox();
                            self.push_spacer();
                            self.state.element_stack.pop();
                        }
                        "li" => {
                            // Pop pending anchor if nothing consumed it
                            let _ = self.state.pending_anchor.take();

                            self.push_current_textbox();
                        }
                        // FIXME: `input` is self closing. This never gets called
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
                            self.state.span_bg =
                                native_color(self.theme.code_color, &self.surface_format);
                            self.state.span_weight = FontWeight::default();
                            self.state.span_style = FontStyle::default();
                            self.state.span_decor = TextDecoration::default();
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
                    if let Some(prefix) = self.state.pending_list_prefix.take() {
                        if self.current_textbox.texts.is_empty() {
                            self.current_textbox.texts.push(
                                Text::new(
                                    prefix,
                                    self.hidpi_scale,
                                    native_color(self.theme.text_color, &self.surface_format),
                                )
                                .make_bold(true),
                            );
                        }
                    }
                    if self.state.text_options.block_quote >= 1 {
                        self.current_textbox
                            .set_quote_block(Some(self.state.text_options.block_quote));
                    }
                    if self.state.text_options.code >= 1 {
                        text = text
                            .with_color(self.state.span_color)
                            .with_bg_color(self.state.span_bg)
                            .with_family(FamilyOwned::Monospace)
                            .make_bold(self.state.span_weight == FontWeight::Bold)
                            .make_italic(self.state.span_style == FontStyle::Italic)
                            .make_underlined(self.state.span_decor == TextDecoration::Underline);
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
                self.window.finished_single_doc();
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
