mod html;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::slice;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{mpsc, Arc, Mutex};

use crate::color::{native_color, Theme};
use crate::image::{Image, ImageData, ImageSize};
use crate::positioner::{Positioned, Row, Section, Spacer, DEFAULT_MARGIN};
use crate::text::{Text, TextBox};
use crate::utils::{markdown_to_html, Align};
use crate::{Element, ImageCache, InlyneEvent};
use html::{
    attr,
    style::{self, FontStyle, FontWeight, Style, TextDecoration},
    Attr, Element as InterpreterElement, TagName,
};

use comrak::Anchorizer;
use glyphon::FamilyOwned;
use html5ever::tendril::*;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use wgpu::TextureFormat;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use self::html::HeaderType;

struct State {
    global_indent: f32,
    element_stack: Vec<InterpreterElement>,
    text_options: html::TextOptions,
    span: Span,
    // Stores the row and a counter of newlines after each image
    inline_images: Option<(Row, usize)>,
    pending_anchor: Option<String>,
    pending_list_prefix: Option<String>,
    anchorizer: Anchorizer,
}

impl State {
    fn with_span_color(span_color: [f32; 4]) -> Self {
        Self {
            global_indent: 0.0,
            element_stack: Vec::new(),
            text_options: Default::default(),
            span: Span::with_color(span_color),
            inline_images: None,
            pending_anchor: None,
            pending_list_prefix: None,
            anchorizer: Default::default(),
        }
    }

    fn element_iter_mut(&mut self) -> slice::IterMut<'_, InterpreterElement> {
        self.element_stack.iter_mut()
    }
}

struct Span {
    color: [f32; 4],
    weight: FontWeight,
    style: FontStyle,
    decor: TextDecoration,
}

impl Span {
    fn with_color(color: [f32; 4]) -> Self {
        Self {
            color,
            weight: Default::default(),
            style: Default::default(),
            decor: Default::default(),
        }
    }
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
            state: State::with_span_color(native_color(theme.code_color, &surface_format)),
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

        let span_color = self.native_color(self.theme.text_color);
        let code_highlighter = self.theme.code_highlighter.clone();
        let mut tok = Tokenizer::new(self, TokenizerOpts::default());

        for md_string in receiver {
            tracing::debug!(
                "Received markdown for interpretation: {} bytes",
                md_string.len()
            );

            if tok.sink.should_queue.load(AtomicOrdering::Relaxed) {
                tok.sink.state = State::with_span_color(span_color);
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

    fn align_or_inherit(&self, maybe_align: Option<Align>) -> Option<Align> {
        maybe_align.or_else(|| self.find_current_align())
    }

    // Searches the currently nested elements for align attribute
    fn find_current_align(&self) -> Option<Align> {
        for element in self.state.element_stack.iter().rev() {
            if let InterpreterElement::Div(Some(elem_align))
            | InterpreterElement::Paragraph(Some(elem_align))
            | InterpreterElement::Header(html::Header {
                align: Some(elem_align),
                ..
            }) = element
            {
                return Some(*elem_align);
            }
        }
        None
    }

    #[must_use]
    fn native_color(&self, color: u32) -> [f32; 4] {
        native_color(color, &self.surface_format)
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
                let section = self.state.element_iter_mut().rev().find_map(|e| {
                    if let InterpreterElement::Details(section) = e {
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
        self.push_element(Spacer::invisible());
    }
    fn push_element<I: Into<Element>>(&mut self, element: I) {
        self.element_queue.lock().unwrap().push_back(element.into());
        if self.first_pass {
            self.window.request_redraw()
        }
    }

    fn process_start_tag(&mut self, tag: Tag) {
        let tag_name = match TagName::try_from(&tag.name) {
            Ok(name) => name,
            Err(name) => {
                tracing::info!("Missing implementation for start tag: {name}");
                return;
            }
        };
        match tag_name {
            TagName::BlockQuote => {
                // FIXME blockquotes in list have no marker
                self.push_current_textbox();
                self.state.text_options.block_quote += 1;
                self.state.global_indent += DEFAULT_MARGIN / 2.;
                self.current_textbox
                    .set_quote_block(self.state.text_options.block_quote);
            }
            TagName::TableHead | TagName::TableBody => {}
            TagName::Table => {
                self.push_spacer();
                self.state.element_stack.push(InterpreterElement::table());
            }
            TagName::TableHeader => {
                self.state.text_options.bold += 1;
                let align = html::find_align(&tag.attrs);
                self.current_textbox.set_align_or_default(align);
            }
            TagName::TableRow => self
                .state
                .element_stack
                .push(InterpreterElement::table_row()),
            TagName::TableDataCell => {
                let align = html::find_align(&tag.attrs);
                self.current_textbox.set_align_or_default(align);
            }
            TagName::Anchor => {
                for attr in attr::Iter::new(&tag.attrs) {
                    match attr {
                        Attr::Href(link) => self.state.text_options.link.push(link),
                        Attr::Anchor(a) => self.current_textbox.set_anchor(a),
                        _ => {}
                    }
                }
            }
            TagName::Small => self.state.text_options.small += 1,
            TagName::Break => self.push_current_textbox(),
            TagName::Underline => self.state.text_options.underline += 1,
            TagName::Strikethrough => self.state.text_options.strike_through += 1,
            TagName::Image => {
                let mut align = None;
                let mut size = None;
                let mut src = None;
                for attr in attr::Iter::new(&tag.attrs) {
                    match attr {
                        Attr::Align(a) => align = Some(a),
                        Attr::Width(w) => size = Some(ImageSize::width(w)),
                        Attr::Height(h) => size = Some(ImageSize::height(h)),
                        Attr::Src(s) => src = Some(s),
                        _ => {}
                    }
                }
                align = self.align_or_inherit(align);
                if let Some(src) = src {
                    let align = align.as_ref().unwrap_or(&Align::Left);
                    let is_url = src.starts_with("http://") || src.starts_with("https://");
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
                            self.state.inline_images =
                                Some((Row::with_image(image, self.hidpi_scale), 1));
                        }
                    } else {
                        self.push_element(image);
                        self.push_spacer();
                    }
                }
            }
            TagName::Div | TagName::Paragraph => {
                self.push_current_textbox();

                // Push potentially pending anchor from containing li
                let anchor_name = self.state.pending_anchor.take();
                if let Some(anchor) = anchor_name {
                    let anchorized = self.state.anchorizer.anchorize(anchor);
                    self.current_textbox.set_anchor(format!("#{anchorized}"));
                }

                let align = html::find_align(&tag.attrs);
                if let Some(align) = self.align_or_inherit(align) {
                    self.current_textbox.set_align(align);
                }
                self.state.element_stack.push(match tag_name {
                    TagName::Div => InterpreterElement::Div(align),
                    TagName::Paragraph => InterpreterElement::Paragraph(align),
                    _ => unreachable!("Arm matches on Div and Paragraph"),
                });
            }
            TagName::EmphasisOrItalic => self.state.text_options.italic += 1,
            TagName::BoldOrStrong => self.state.text_options.bold += 1,
            TagName::Code => self.state.text_options.code += 1,
            TagName::ListItem => {
                for attr in attr::Iter::new(&tag.attrs) {
                    self.state.pending_anchor = attr.to_anchor();
                }

                // Push a pending list prefix based on the list type
                let iter = self.state.element_iter_mut();
                let list = iter.rev().find_map(|elem| elem.as_mut_list()).unwrap();
                if self.current_textbox.texts.is_empty() {
                    let prefix = match &mut list.ty {
                        html::ListType::Ordered(index) => {
                            *index += 1;
                            format!("{}. ", *index - 1)
                        }
                        html::ListType::Unordered => "· ".to_owned(),
                    };

                    self.state.pending_list_prefix = Some(prefix);
                }
            }
            TagName::UnorderedList => {
                self.push_current_textbox();
                self.state.global_indent += DEFAULT_MARGIN / 2.;
                self.state
                    .element_stack
                    .push(InterpreterElement::unordered_list());
            }
            TagName::OrderedList => {
                let mut start_index = 1;
                for attr in attr::Iter::new(&tag.attrs) {
                    if let Attr::Start(start) = attr {
                        start_index = start;
                    }
                }
                self.push_current_textbox();
                self.state.global_indent += DEFAULT_MARGIN / 2.;
                self.state
                    .element_stack
                    .push(InterpreterElement::ordered_list(start_index));
            }
            TagName::Header(header_type) => {
                let align = html::find_align(&tag.attrs);
                self.push_current_textbox();
                self.push_spacer();
                if let html::HeaderType::H1 = header_type {
                    self.state.text_options.underline += 1;
                }
                self.state
                    .element_stack
                    .push(InterpreterElement::Header(html::Header::new(
                        header_type,
                        align,
                    )));
                self.current_textbox.set_align_or_default(align);
            }
            TagName::PreformattedText => {
                self.push_current_textbox();
                let style_str = html::find_style(&tag.attrs).unwrap_or_default();
                for style in style::Iter::new(&style_str) {
                    if let Style::BackgroundColor(color) = style {
                        let native_color = self.native_color(color);
                        self.current_textbox.set_background_color(native_color);
                    }
                }
                self.state.text_options.pre_formatted += 1;
                self.current_textbox.set_code_block(true);
            }
            // HACK: spans are only supported enough to get syntax highlighting in code
            // blocks working
            TagName::Span => {
                let style_str = html::find_style(&tag.attrs).unwrap_or_default();
                for style in style::Iter::new(&style_str) {
                    match style {
                        Style::Color(color) => {
                            self.state.span.color = native_color(color, &self.surface_format)
                        }
                        Style::FontWeight(weight) => self.state.span.weight = weight,
                        Style::FontStyle(style) => self.state.span.style = style,
                        Style::TextDecoration(decor) => self.state.span.decor = decor,
                        _ => {}
                    }
                }
            }
            TagName::Input => {
                let mut is_checkbox = false;
                let mut is_checked = false;
                for attr in attr::Iter::new(&tag.attrs) {
                    match attr {
                        Attr::IsCheckbox => is_checkbox = true,
                        Attr::IsChecked => is_checked = true,
                        _ => {}
                    }
                }
                if is_checkbox {
                    // Checkbox uses a custom prefix, so remove pending text prefix
                    let _ = self.state.pending_list_prefix.take();
                    self.current_textbox.set_checkbox(is_checked);
                    self.state.element_stack.push(InterpreterElement::Input);
                }
            }
            TagName::Details => {
                self.push_current_textbox();
                self.push_spacer();
                let section = Section::bare(self.hidpi_scale);
                *section.hidden.borrow_mut() = true;
                self.state
                    .element_stack
                    .push(InterpreterElement::Details(section));
            }
            TagName::Summary => {
                self.push_current_textbox();
                self.state.element_stack.push(InterpreterElement::Summary);
            }
            TagName::HorizontalRuler => {
                self.push_element(Spacer::visible());
            }
            TagName::Section => {}
        }
    }

    fn process_end_tag(&mut self, tag: Tag) {
        let tag_name = match TagName::try_from(&tag.name) {
            Ok(name) => name,
            Err(name) => {
                tracing::info!("Missing implementation for end tag: {name}");
                return;
            }
        };
        match tag_name {
            TagName::Underline => self.state.text_options.underline -= 1,
            TagName::Strikethrough => self.state.text_options.strike_through -= 1,
            TagName::Small => self.state.text_options.small -= 1,
            TagName::TableHead | TagName::TableBody => {}
            TagName::TableHeader => {
                let iter = self.state.element_iter_mut();
                let table = iter.rev().find_map(|elem| elem.as_mut_table()).unwrap();
                table.push_header(self.current_textbox.clone());
                self.current_textbox.texts.clear();
                self.state.text_options.bold -= 1;
            }
            TagName::TableDataCell => {
                let table_row = self.state.element_stack.last_mut();
                if let Some(InterpreterElement::TableRow(ref mut row)) = table_row {
                    row.push(self.current_textbox.clone());
                }
                self.current_textbox.texts.clear();
            }
            TagName::TableRow => {
                let table_row = self.state.element_stack.pop();
                for mut element in self.state.element_iter_mut().rev() {
                    if let InterpreterElement::Table(table) = &mut element {
                        if let Some(InterpreterElement::TableRow(row)) = table_row {
                            if !row.is_empty() {
                                table.push_row(row);
                            }
                            break;
                        }
                    }
                }
                self.current_textbox.texts.clear();
            }
            TagName::Table => {
                if let Some(InterpreterElement::Table(table)) = self.state.element_stack.pop() {
                    self.push_element(table);
                    self.push_spacer();
                }
            }
            TagName::Anchor => {
                self.state.text_options.link.pop();
            }
            TagName::Code => self.state.text_options.code -= 1,
            TagName::Div | TagName::Paragraph => {
                self.push_current_textbox();
                if tag_name == TagName::Paragraph {
                    self.push_spacer();
                }
                self.state.element_stack.pop();
            }
            TagName::EmphasisOrItalic => self.state.text_options.italic -= 1,
            TagName::BoldOrStrong => self.state.text_options.bold -= 1,
            TagName::Header(header_type) => {
                if header_type == HeaderType::H1 {
                    self.state.text_options.underline -= 1;
                }
                let anchor_name = self
                    .current_textbox
                    .texts
                    .iter()
                    .flat_map(|t| t.text.chars())
                    .collect();
                let anchorized = self.state.anchorizer.anchorize(anchor_name);
                self.current_textbox.set_anchor(format!("#{anchorized}"));
                self.push_current_textbox();
                self.push_spacer();
                self.state.element_stack.pop();
            }
            TagName::ListItem => {
                // Pop pending anchor if nothing consumed it
                let _ = self.state.pending_anchor.take();

                self.push_current_textbox();
            }
            // FIXME: `input` is self closing. This never gets called
            TagName::Input => {
                self.push_current_textbox();
                self.state.element_stack.pop();
            }
            TagName::UnorderedList | TagName::OrderedList => {
                self.push_current_textbox();
                self.state.global_indent -= DEFAULT_MARGIN / 2.;
                self.state.element_stack.pop();
                if self.state.global_indent == 0. {
                    self.push_spacer();
                }
            }
            TagName::PreformattedText => {
                self.push_current_textbox();
                self.push_spacer();
                self.state.text_options.pre_formatted -= 1;
                self.current_textbox.set_code_block(false);
            }
            TagName::BlockQuote => {
                self.push_current_textbox();
                self.state.text_options.block_quote -= 1;
                self.state.global_indent -= DEFAULT_MARGIN / 2.;
                self.current_textbox.clear_quote_block();
                if self.state.global_indent == 0. {
                    self.push_spacer();
                }
            }
            TagName::Span => {
                let color = self.native_color(self.theme.code_color);
                self.state.span = Span::with_color(color);
            }
            TagName::Details => {
                self.push_current_textbox();
                if let Some(InterpreterElement::Details(section)) = self.state.element_stack.pop() {
                    self.push_element(section);
                }
                self.push_spacer();
            }
            TagName::Summary => {
                for mut element in self.state.element_iter_mut().rev() {
                    if let InterpreterElement::Details(section) = &mut element {
                        *section.summary = Some(Positioned::new(self.current_textbox.clone()));
                        self.current_textbox.texts.clear();
                        break;
                    }
                }
                self.state.element_stack.pop();
            }
            TagName::HorizontalRuler | TagName::Break | TagName::Image | TagName::Section => {}
        }
    }

    fn process_character_tokens(&mut self, mut str: String) {
        let text_native_color = self.native_color(self.theme.text_color);
        if str == "\n" {
            if self.state.text_options.pre_formatted >= 1 {
                self.current_textbox.texts.push(Text::new(
                    "\n".to_string(),
                    self.hidpi_scale,
                    text_native_color,
                ));
            }
            if let Some(last_text) = self.current_textbox.texts.last() {
                if let Some(last_char) = last_text.text.chars().last() {
                    if !last_char.is_whitespace() {
                        self.current_textbox.texts.push(Text::new(
                            " ".to_string(),
                            self.hidpi_scale,
                            text_native_color,
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
                            text_native_color,
                        ));
                    }
                }
            }
        } else {
            if self.current_textbox.texts.is_empty() && self.state.text_options.pre_formatted == 0 {
                str = str.trim_start().to_owned();
            }

            let mut text = Text::new(str, self.hidpi_scale, text_native_color);
            if let Some(prefix) = self.state.pending_list_prefix.take() {
                if self.current_textbox.texts.is_empty() {
                    self.current_textbox.texts.push(
                        Text::new(prefix, self.hidpi_scale, text_native_color).make_bold(true),
                    );
                }
            }
            if self.state.text_options.block_quote >= 1 {
                self.current_textbox
                    .set_quote_block(self.state.text_options.block_quote);
            }
            if self.state.text_options.code >= 1 {
                text = text
                    .with_color(self.state.span.color)
                    .with_family(FamilyOwned::Monospace);
                if self.state.span.weight == FontWeight::Bold {
                    text = text.make_bold(true);
                }
                if self.state.span.style == FontStyle::Italic {
                    text = text.make_italic(true);
                }
                if self.state.span.decor == TextDecoration::Underline {
                    text = text.make_underlined(true);
                }
            }
            for elem in self.state.element_stack.iter().rev() {
                if let InterpreterElement::Header(header) = elem {
                    self.current_textbox.font_size = header.ty.text_size();
                    text = text.make_bold(true);
                    break;
                }
            }
            if let Some(link) = self.state.text_options.link.last() {
                text = text.with_link((*link).clone());
                text = text.with_color(self.native_color(self.theme.link_color));
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
            }
            self.current_textbox.texts.push(text);
        }
    }
}

impl TokenSink for HtmlInterpreter {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        if !self.should_queue.load(AtomicOrdering::Relaxed) {
            self.stopped = true;
        }
        if self.stopped {
            return TokenSinkResult::Continue;
        }
        match token {
            Token::TagToken(tag) => match tag.kind {
                TagKind::StartTag => self.process_start_tag(tag),
                TagKind::EndTag => self.process_end_tag(tag),
            },
            Token::CharacterTokens(str) => self.process_character_tokens(str.to_string()),
            Token::EOFToken => {
                self.push_current_textbox();
                self.should_queue.store(false, AtomicOrdering::Relaxed);
                self.first_pass = false;
                self.window.finished_single_doc();
            }
            Token::ParseError(err) => tracing::warn!("HTML parser emitted error: {err}"),
            Token::DoctypeToken(_) | Token::CommentToken(_) | Token::NullCharacterToken => {}
        }
        TokenSinkResult::Continue
    }
}
