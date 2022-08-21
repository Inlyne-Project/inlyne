use crate::color::hex_to_linear_rgba;
use crate::image::Image;
use crate::image::ImageSize;
use crate::positioner::Positioned;
use crate::positioner::Row;
use crate::positioner::Spacer;
use crate::positioner::DEFAULT_MARGIN;
use crate::table::Table;

use crate::color::Theme;
use crate::text::{Text, TextBox};
use crate::utils::Align;
use crate::Element;

use comrak::{markdown_to_html_with_plugins, ComrakOptions};
use html5ever::local_name;
use html5ever::tendril::*;
use html5ever::tokenizer::BufferQueue;
use html5ever::tokenizer::TagKind;
use html5ever::tokenizer::TagToken;
use html5ever::tokenizer::{Token, TokenSink, TokenSinkResult};
use html5ever::tokenizer::{Tokenizer, TokenizerOpts};
use html5ever::Attribute;
use winit::window::Window;
use Token::{CharacterTokens, EOFToken};

use std::collections::VecDeque;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

mod html {
    use crate::{table::Table, text::TextBox, utils::Align};

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
        pub allow_whitespace: usize,
        pub block_quote: usize,
        pub link: Vec<String>,
    }

    pub enum Element {
        List(List),
        ListItem,
        Table(Table),
        TableRow(Vec<TextBox>),
        Header(Header),
        Paragraph(Option<Align>),
        Div(Option<Align>),
    }
}

#[derive(Default)]
struct State {
    global_indent: f32,
    element_stack: Vec<html::Element>,
    text_options: html::TextOptions,
    span_color: [f32; 4],
    // Stores the row and a counter of newlines after each image
    inline_images: Option<(Row, usize)>,
}

pub struct HtmlInterpreter {
    element_queue: Arc<Mutex<VecDeque<Element>>>,
    current_textbox: TextBox,
    hidpi_scale: f32,
    theme: Theme,
    window: Arc<Window>,
    state: State,
    file_path: PathBuf,
}

impl HtmlInterpreter {
    pub fn new(
        window: Arc<Window>,
        element_queue: Arc<Mutex<VecDeque<Element>>>,
        theme: Theme,
        hidpi_scale: f32,
        file_path: PathBuf,
    ) -> Self {
        Self {
            window,
            element_queue,
            current_textbox: TextBox::new(Vec::new(), hidpi_scale),
            hidpi_scale,
            state: State {
                span_color: theme.code_color,
                ..Default::default()
            },
            theme,
            file_path,
        }
    }

    pub fn intepret_md(self, md_string: &str) {
        let mut input = BufferQueue::new();
        let mut options = ComrakOptions::default();
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.parse.smart = true;
        options.render.unsafe_ = true;

        let mut plugins = comrak::ComrakPlugins::default();
        let adapter = comrak::plugins::syntect::SyntectAdapter::new(
            self.theme.code_highlighter.as_syntect_name(),
        );
        plugins.render.codefence_syntax_highlighter = Some(&adapter);

        let htmlified = markdown_to_html_with_plugins(md_string, &options, &plugins);

        input.push_back(
            Tendril::from_str(&htmlified)
                .unwrap()
                .try_reinterpret::<fmt::UTF8>()
                .unwrap(),
        );

        let mut tok = Tokenizer::new(self, TokenizerOpts::default());
        let _ = tok.feed(&mut input);
        assert!(input.is_empty());
        tok.end();
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
                self.push_element(row.into());
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
                self.push_element(self.current_textbox.clone().into());
            }
        }
        self.current_textbox = TextBox::new(Vec::new(), self.hidpi_scale);
    }
    fn push_spacer(&mut self) {
        self.push_element(Spacer::new(5.).into());
    }
    fn push_element(&mut self, element: Element) {
        self.element_queue.lock().unwrap().push_back(element);
        self.window.request_redraw()
    }
}

impl TokenSink for HtmlInterpreter {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
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
                            self.current_textbox.indent = self.state.global_indent;
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
                            let attrs = tag.attrs;
                            for attr in attrs {
                                if attr.name.local == local_name!("href") {
                                    self.state.text_options.link.push(attr.value.to_string());
                                    break;
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
                                    let mut image = Image::from_url(
                                        attr.value.to_string(),
                                        self.file_path.clone(),
                                        self.hidpi_scale,
                                    )
                                    .with_align(*align);
                                    if let Some(link) = self.state.text_options.link.last() {
                                        image.set_link((*link).clone())
                                    }
                                    if let Some(size) = size {
                                        image = image.with_size(size);
                                    }

                                    if align == &Align::Left {
                                        if let Some((row, count)) = &mut self.state.inline_images {
                                            row.elements.push(Positioned::new(image.into()));
                                            // Restart newline count
                                            *count = 1;
                                        } else {
                                            self.state.inline_images = Some((
                                                Row::new(
                                                    vec![Positioned::new(image.into())],
                                                    self.hidpi_scale,
                                                ),
                                                1,
                                            ));
                                        }
                                    } else {
                                        self.push_element(image.into());
                                        self.push_spacer();
                                    }
                                    break;
                                }
                            }
                        }
                        "div" | "p" => {
                            self.push_current_textbox();
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
                                "p" => {
                                    self.state.text_options.allow_whitespace += 1;
                                    html::Element::Paragraph(align)
                                }
                                _ => unreachable!("Arm matches on div and p"),
                            });
                        }
                        "em" | "i" => self.state.text_options.italic += 1,
                        "bold" | "strong" => self.state.text_options.bold += 1,
                        "code" => self.state.text_options.code += 1,
                        "li" => {
                            self.current_textbox.indent = self.state.global_indent;
                            self.state.element_stack.push(html::Element::ListItem);
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
                            self.current_textbox.indent = self.state.global_indent;
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
                            self.state.text_options.allow_whitespace += 1;
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
                                            let bg_color = hex_to_linear_rgba(hex);
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
                                    if let Some(hex_str) = styles
                                        .split(';')
                                        .find_map(|style| style.strip_prefix("color:#"))
                                    {
                                        if let Ok(hex) = u32::from_str_radix(hex_str, 16) {
                                            self.state.span_color = hex_to_linear_rgba(hex);
                                        }
                                    }
                                }
                            }
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
                                self.push_element(table.into());
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
                                self.state.text_options.allow_whitespace -= 1;
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
                            self.state.text_options.allow_whitespace -= 1;
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
                            self.push_current_textbox();
                            self.state.element_stack.pop();
                        }
                        "ul" | "ol" => {
                            self.push_current_textbox();
                            self.state.global_indent -= DEFAULT_MARGIN / 2.;
                            self.current_textbox.indent = self.state.global_indent;
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
                            self.current_textbox.indent = self.state.global_indent;
                            self.current_textbox.set_quote_block(None);
                            if self.state.global_indent == 0. {
                                self.push_spacer();
                            }
                        }
                        "span" => self.state.span_color = self.theme.code_color,
                        _ => {}
                    },
                }
            }
            CharacterTokens(str) => {
                let mut str = str.to_string();
                if str == "\n" {
                    if self.state.text_options.pre_formatted >= 1 {
                        if !self.current_textbox.texts.is_empty() {
                            self.push_element(self.current_textbox.clone().into());
                            self.current_textbox.texts.clear();
                        } else {
                            self.push_element(self.current_textbox.clone().with_padding(12.).into())
                        }
                    }
                    if let Some(last_text) = self.current_textbox.texts.last() {
                        if !last_text.text.trim().is_empty() {
                            self.current_textbox.texts.push(Text::new(
                                " ".to_string(),
                                self.hidpi_scale,
                                self.theme.text_color,
                            ));
                        }
                    }
                    if let Some((row, newline_counter)) = self.state.inline_images.take() {
                        if newline_counter == 0 {
                            self.push_element(row.into());
                            self.push_spacer();
                        } else {
                            self.state.inline_images = Some((row, newline_counter - 1));
                        }
                    }
                } else if !str.trim().is_empty() || self.state.text_options.pre_formatted >= 1 {
                    if self.state.text_options.allow_whitespace == 0
                        && self.state.text_options.pre_formatted == 0
                    {
                        str = str.trim().to_owned();
                    }
                    let mut text = Text::new(str, self.hidpi_scale, self.theme.text_color);
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
                                        self.theme.text_color,
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
                                        self.theme.text_color,
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
                            .with_font(1)
                            .with_size(18.)
                    }
                    for elem in self.state.element_stack.iter().rev() {
                        if let html::Element::Header(header) = elem {
                            text = text
                                .with_size(header.header_type.text_size())
                                .make_bold(true);
                            break;
                        }
                    }
                    if let Some(link) = self.state.text_options.link.last() {
                        text = text.with_link((*link).clone());
                        text = text.with_color(self.theme.link_color);
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
                        text = text.with_size(12.);
                    }
                    self.current_textbox.texts.push(text);
                }
            }
            EOFToken => {
                self.push_current_textbox();
                self.window.request_redraw();
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
