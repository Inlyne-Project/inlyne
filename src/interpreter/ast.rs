use crate::color::{native_color, Theme};
use crate::interpreter::hir::{unwrap_hir_node, Hir, HirNode, TextOrHirNode};
use crate::interpreter::html::style::{FontStyle, FontWeight, Style, TextDecoration};
use crate::interpreter::html::{style, Attr, HeaderType, TagName};
use crate::interpreter::Span;
use crate::positioner::{Section, Spacer, DEFAULT_MARGIN};
use crate::table::Table;
use crate::text::{Text, TextBox};
use crate::utils::Align;
use crate::Element;
use comrak::Anchorizer;
use glyphon::FamilyOwned;
use std::cell::{Cell, RefCell};
use std::num::NonZeroU8;
use std::ops::DerefMut;
use wgpu::TextureFormat;

#[derive(Debug, Copy, Clone, Default)]
struct TextOptions {
    pub underline: bool,
    pub bold: bool,
    pub italic: bool,
    pub strike_through: bool,
    pub small: bool,
    pub code: bool,
    pub pre_formatted: bool,
    pub block_quote: u8,
    pub align: Option<Align>,
}

#[derive(Debug, Copy, Clone, Default)]
struct InheritedState {
    global_indent: f32,
    text_options: TextOptions,
    span: Span,

    /// Li render as ether as "· " or as an "{1..}. ".
    list_prefix: Option<Option<NonZeroU8>>,
}
impl InheritedState {
    fn set_align(&mut self, align: Option<Align>) {
        self.text_options.align = align.or(self.text_options.align);
    }
}

type Content = Vec<TextOrHirNode>;

pub(crate) struct Ast {
    pub ast: Vec<Element>,
    pub anchorizer: Anchorizer,
    pub theme: Theme,
    pub current_textbox: RefCell<TextBox>,
    pub hidpi_scale: f32,
    pub surface_format: TextureFormat,
    pub link: Cell<Option<String>>,
}
impl Ast {
    pub fn new() -> Self {
        Self {
            ast: Vec::new(),
            anchorizer: Default::default(),
            current_textbox: Default::default(),
            hidpi_scale: Default::default(),
            theme: Theme::dark_default(),
            surface_format: TextureFormat::Bgra8UnormSrgb,
            link: Cell::new(None),
        }
    }
    pub fn interpret(mut self, hir: Hir) -> Self {
        let content = hir.content();
        self.process_content(Default::default(), content);
        self
    }
    pub fn into_inner(self) -> Vec<Element> {
        self.ast
    }

    fn process_content(&mut self, inherited_state: InheritedState, content: Content) {
        for node in content {
            match node {
                TextOrHirNode::Text(str) => self.text(
                    self.current_textbox.borrow_mut().deref_mut(),
                    inherited_state,
                    str,
                ),
                TextOrHirNode::Hir(node) => {
                    self.process_node(inherited_state, unwrap_hir_node(node))
                }
            }
        }
    }

    fn process_node(&mut self, mut inherited_state: InheritedState, node: HirNode) {
        let content = node.content;
        let attributes = node.attributes;

        match node.tag {
            TagName::Paragraph => {
                self.push_text_box(inherited_state);

                self.process_content(inherited_state, content);

                self.push_text_box(inherited_state);
                self.push_spacer();
            }
            TagName::Anchor => {
                for attr in attributes {
                    match attr {
                        Attr::Href(link) => self.link.set(Some(link)),
                        Attr::Anchor(a) => self.current_textbox.borrow_mut().set_anchor(a),
                        _ => {}
                    }
                }
                self.process_content(inherited_state, content);
            }
            TagName::Div => {
                self.push_text_box(inherited_state);
                self.process_content(inherited_state, content);
                self.push_text_box(inherited_state);
            }
            TagName::BlockQuote => {
                self.push_text_box(inherited_state);
                inherited_state.text_options.block_quote += 1;
                inherited_state.global_indent += DEFAULT_MARGIN / 2.;

                self.process_content(inherited_state, content);

                self.push_text_box(inherited_state);
                if inherited_state.global_indent == DEFAULT_MARGIN / 2. {
                    self.push_spacer();
                }
            }
            TagName::BoldOrStrong => {
                inherited_state.text_options.bold = true;
                self.process_content(inherited_state, content);
            }
            TagName::Break => {
                self.push_text_box(inherited_state);
                self.process_content(inherited_state, content);
            }
            TagName::Code => {
                inherited_state.text_options.code = true;
                self.process_content(inherited_state, content);
            }
            TagName::Details => {
                return;
                self.push_text_box(inherited_state);
                self.push_spacer();
                let section = Section::bare(self.hidpi_scale);
                *section.hidden.borrow_mut() = true;
                todo!("Details Implementation");
                self.push_element(section);
            }
            TagName::Section => {}
            TagName::Summary => tracing::warn!("Summary can only be in an Details element"),
            TagName::EmphasisOrItalic => {
                inherited_state.text_options.italic = true;
                self.process_content(inherited_state, content);
            }
            TagName::Header(header) => {
                self.push_text_box(inherited_state);
                self.push_spacer();

                inherited_state.set_align(attributes.iter().find_map(|attr| attr.to_align()));
                inherited_state.text_options.bold = true;
                self.current_textbox.borrow_mut().font_size *= header.size_multiplier();

                if header == HeaderType::H1 {
                    inherited_state.text_options.underline = true;
                }
                self.process_content(inherited_state, content);

                let anchor = self
                    .current_textbox
                    .borrow()
                    .texts
                    .iter()
                    .flat_map(|t| t.text.chars())
                    .collect();
                let anchor = self.anchorizer.anchorize(anchor);
                self.current_textbox
                    .borrow_mut()
                    .set_anchor(format!("#{anchor}"));
                self.push_text_box(inherited_state);
                self.push_spacer();
            }
            TagName::HorizontalRuler => {
                self.push_element(Spacer::visible());
                self.process_content(inherited_state, content);
            }
            TagName::Picture => tracing::warn!("No picture impl"),
            TagName::Source => tracing::warn!("No source impl"),
            TagName::Image => tracing::warn!("No image impl"),
            TagName::Input => {
                let mut is_checkbox = false;
                let mut is_checked = false;
                for attr in attributes {
                    match attr {
                        Attr::IsCheckbox => is_checkbox = true,
                        Attr::IsChecked => is_checked = true,
                        _ => {}
                    }
                }
                if is_checkbox {
                    self.current_textbox.borrow_mut().set_checkbox(is_checked);
                }
                self.process_content(inherited_state, content);
            }
            TagName::ListItem => {}
            TagName::OrderedList => {}
            TagName::UnorderedList => {}
            TagName::PreformattedText => {
                self.push_text_box(inherited_state);
                let style = attributes
                    .iter()
                    .find_map(|attr| attr.to_style())
                    .unwrap_or_default();
                for style in style::Iter::new(&style) {
                    if let Style::BackgroundColor(color) = style {
                        let native_color = self.native_color(color);
                        self.current_textbox
                            .borrow_mut()
                            .set_background_color(native_color);
                    }
                }
                inherited_state.text_options.pre_formatted = true;
                self.current_textbox.borrow_mut().set_code_block(true);
                self.process_content(inherited_state, content);

                self.push_text_box(inherited_state);

                self.push_spacer();
                inherited_state.text_options.pre_formatted = false;
                self.current_textbox.borrow_mut().set_code_block(false);
            }
            TagName::Small => {
                inherited_state.text_options.small = true;
                self.process_content(inherited_state, content);
            }
            TagName::Span => {
                let style_str = attributes
                    .iter()
                    .find_map(|attr| attr.to_style())
                    .unwrap_or_default();
                for style in style::Iter::new(&style_str) {
                    match style {
                        Style::Color(color) => {
                            inherited_state.span.color = native_color(color, &self.surface_format)
                        }
                        Style::FontWeight(weight) => inherited_state.span.weight = weight,
                        Style::FontStyle(style) => inherited_state.span.style = style,
                        Style::TextDecoration(decor) => inherited_state.span.decor = decor,
                        _ => {}
                    }
                }
                self.process_content(inherited_state, content);
            }
            TagName::Strikethrough => {
                inherited_state.text_options.strike_through = true;
                self.process_content(inherited_state, content);
            }
            TagName::Table => {
                let mut table = Table::new();
                self.process_table(&mut table, inherited_state, content);
                self.push_element(table);
            }
            TagName::TableHead | TagName::TableBody => {
                tracing::warn!("TableHead and TableBody not supported");
            }
            TagName::TableRow => tracing::warn!("Summary can only be in an Table element"),
            TagName::TableDataCell => {
                tracing::warn!("Summary can only be in an TableRow or an TableHeader element");
            }
            TagName::TableHeader => tracing::warn!("Summary can only be in an TableRow element"),
            TagName::Underline => {
                inherited_state.text_options.underline = true;
                self.process_content(inherited_state, content);
            }
            TagName::Root => tracing::error!("Root element can't reach interpreter."),
        }
    }

    fn text(&self, text_box: &mut TextBox, state: InheritedState, mut string: String) {
        let text_native_color = self.native_color(self.theme.text_color);
        if string == "\n" {
            if state.text_options.pre_formatted {
                text_box.texts.push(Text::new(
                    "\n".to_string(),
                    self.hidpi_scale,
                    text_native_color,
                ));
            }
            if let Some(last_text) = text_box.texts.last() {
                if let Some(last_char) = last_text.text.chars().last() {
                    if !last_char.is_whitespace() {
                        text_box.texts.push(Text::new(
                            " ".to_string(),
                            self.hidpi_scale,
                            text_native_color,
                        ));
                    }
                }
            }
        } else if string.trim().is_empty() && !state.text_options.pre_formatted {
            if let Some(last_text) = text_box.texts.last() {
                if let Some(last_char) = last_text.text.chars().last() {
                    if !last_char.is_whitespace() {
                        text_box.texts.push(Text::new(
                            " ".to_string(),
                            self.hidpi_scale,
                            text_native_color,
                        ));
                    }
                }
            }
        } else {
            if text_box.texts.is_empty() && !state.text_options.pre_formatted {
                #[allow(
                unknown_lints, // Rust is still bad with back compat on new lints
                clippy::assigning_clones // Hit's a borrow-check issue. Needs a different impl
                )]
                {
                    string = string.trim_start().to_owned();
                }
            }

            let mut text = Text::new(string, self.hidpi_scale, text_native_color);
            if state.text_options.block_quote >= 1 {
                text_box.set_quote_block(state.text_options.block_quote as usize);
            }
            if state.text_options.code {
                text = text
                    .with_color(state.span.color)
                    .with_family(FamilyOwned::Monospace);
                if state.span.weight == FontWeight::Bold {
                    text = text.make_bold(true);
                }
                if state.span.style == FontStyle::Italic {
                    text = text.make_italic(true);
                }
                if state.span.decor == TextDecoration::Underline {
                    text = text.make_underlined(true);
                }
            }
            if let Some(link) = self.link.take() {
                text = text.with_link(link.to_string());
                text = text.with_color(self.native_color(self.theme.link_color));
            }
            if state.text_options.bold {
                text = text.make_bold(true);
            }
            if state.text_options.italic {
                text = text.make_italic(true);
            }
            if state.text_options.underline {
                text = text.make_underlined(true);
            }
            if state.text_options.strike_through {
                text = text.make_striked(true);
            }

            if state.text_options.small {
                text_box.font_size = 12.;
            }
            text_box.texts.push(text);
        }
    }

    fn push_element<T: Into<Element>>(&mut self, element: T) {
        self.ast.push(element.into())
    }
    fn push_text_box(&mut self, state: InheritedState) {
        let mut tb = std::mem::replace(
            self.current_textbox.borrow_mut().deref_mut(),
            TextBox::new(vec![], self.hidpi_scale),
        );
        self.current_textbox.borrow_mut().indent = state.global_indent;

        if !tb.texts.is_empty() {
            let content = tb.texts.iter().any(|text| !text.text.is_empty());

            if content {
                tb.indent = state.global_indent;

                self.push_element(tb);
            }
        }
    }

    fn push_spacer(&mut self) {
        self.push_element(Spacer::invisible());
    }

    #[must_use]
    fn native_color(&self, color: u32) -> [f32; 4] {
        native_color(color, &self.surface_format)
    }

    // https://html.spec.whatwg.org/multipage/tables.html#the-table-element
    fn process_table(
        &mut self,
        table: &mut Table,
        inherited_state: InheritedState,
        content: Content,
    ) {
        for node in content {
            let node = if let TextOrHirNode::Hir(node) = node {
                unwrap_hir_node(node)
            } else {
                tracing::warn!("No text node can be in an Table.");
                continue;
            };

            match node.tag {
                TagName::TableHead | TagName::TableBody => {
                    self.process_table_head_body(table, inherited_state, node.content);
                }
                TagName::TableRow => {
                    table.rows.push(vec![]);
                    self.process_table_row(table, inherited_state, node.content)
                }
                _ => {
                    tracing::warn!("Only TableHead, TableBody, TableRow and TableFoot can be inside an table, found: {:?}", node.tag);
                    continue;
                }
            }
        }
        // TODO: filter out empty rows. (without cloning)
    }
    fn process_table_head_body(
        &mut self,
        table: &mut Table,
        inherited_state: InheritedState,
        content: Content,
    ) {
        for node in content {
            let node = if let TextOrHirNode::Hir(node) = node {
                unwrap_hir_node(node)
            } else {
                tracing::warn!("No text node can be in an TableHead or TableBody.");
                continue;
            };

            match node.tag {
                TagName::TableRow => {
                    table.rows.push(vec![]);
                    self.process_table_row(table, inherited_state, node.content)
                }
                _ => {
                    tracing::warn!(
                        "Only TableRows can be inside an TableHead or TableBody, found: {:?}",
                        node.tag
                    );
                    continue;
                }
            }
        }
    }

    // https://html.spec.whatwg.org/multipage/tables.html#the-tr-element
    fn process_table_row(
        &mut self,
        table: &mut Table,
        inherited_state: InheritedState,
        content: Content,
    ) {
        for node in content {
            let node = if let TextOrHirNode::Hir(node) = node {
                unwrap_hir_node(node)
            } else {
                tracing::warn!("No text node can be in an TableRow.");
                continue;
            };

            match node.tag {
                TagName::TableHeader => {
                    self.process_table_header(table, inherited_state, node.content)
                }
                TagName::TableDataCell => {
                    self.process_table_cell(table, inherited_state, node.content)
                }
                _ => {
                    tracing::warn!("Only TableHead, TableBody, TableRow and TableFoot can be inside an table, found: {:?}", node.tag);
                    continue;
                }
            }
        }
    }

    // https://html.spec.whatwg.org/multipage/tables.html#the-th-element
    fn process_table_header(
        &mut self,
        table: &mut Table,
        mut inherited_state: InheritedState,
        content: Content,
    ) {
        let row = table
            .rows
            .last_mut()
            .expect("There should be at least one row.");
        // TODO allow anything inside tables not only text.
        inherited_state.text_options.bold = true;
        for node in content {
            if let TextOrHirNode::Text(text) = node {
                let mut tb = TextBox::new(vec![], self.hidpi_scale);
                self.text(&mut tb, inherited_state, text);
                row.push(tb);
            } else {
                tracing::warn!("Currently only text is allowed in an TableHeader.")
            }
        }
    }

    // https://html.spec.whatwg.org/multipage/tables.html#the-td-element
    fn process_table_cell(
        &mut self,
        table: &mut Table,
        inherited_state: InheritedState,
        content: Content,
    ) {
        let row = table
            .rows
            .last_mut()
            .expect("There should be at least one row.");
        // TODO allow anything inside tables not only text.
        // when doing this make process_node generic over some output so it can be use here

        for node in content {
            if let TextOrHirNode::Text(text) = node {
                let mut tb = TextBox::new(vec![], self.hidpi_scale);
                self.text(&mut tb, inherited_state, text);
                row.push(tb);
            } else {
                tracing::warn!("Currently only text is allowed in an TableDataCell.")
            }
        }
    }
}
