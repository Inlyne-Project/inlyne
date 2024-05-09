use crate::color::{native_color, Theme};
use crate::image::{Image, ImageSize};
use crate::interpreter::hir::{Hir, HirNode, TextOrHirNode};
use crate::interpreter::html::attr::PrefersColorScheme;
use crate::interpreter::html::picture::Builder;
use crate::interpreter::html::style::{FontStyle, FontWeight, Style, TextDecoration};
use crate::interpreter::html::{style, Attr, HeaderType, Picture, TagName};
use crate::interpreter::{Span, WindowInteractor};
use crate::opts::ResolvedTheme;
use crate::positioner::{Positioned, Section, Spacer, DEFAULT_MARGIN};
use crate::table::Table;
use crate::text::{Text, TextBox};
use crate::utils::{Align, ImageCache};
use crate::Element;
use comrak::Anchorizer;
use glyphon::FamilyOwned;
use parking_lot::Mutex;
use percent_encoding::percent_decode_str;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use wgpu::TextureFormat;

#[derive(Debug, Clone, Default)]
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
    pub link: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct InheritedState {
    global_indent: f32,
    text_options: TextOptions,
    span: Span,
}

impl InheritedState {
    fn with_span_color(span_color: [f32; 4]) -> Self {
        Self {
            span: Span::with_color(span_color),
            ..Default::default()
        }
    }
    fn set_align(&mut self, align: Option<Align>) {
        self.text_options.align = align.or(self.text_options.align);
    }
    fn set_align_from_attributes(&mut self, attributes: &[Attr]) {
        self.set_align(attributes.iter().find_map(|attr| attr.to_align()));
    }
}

#[derive(Copy, Clone)]
pub struct Input<'a>(&'a [HirNode]);
impl<'a> Input<'a> {
    fn get(&self, index: usize) -> &'a HirNode {
        self.0
            .get(index)
            .expect("Input should be called with an valid index")
    }
}
type Opts<'a> = &'a AstOpts;

trait OutputStream {
    type Output;
    fn push(&mut self, i: impl Into<Self::Output>);

    fn map<F, O>(&mut self, f: F) -> Map<Self, F, O>
    where
        Self: Sized,
    {
        Map(self, f, PhantomData)
    }
}
impl<T> OutputStream for Vec<T> {
    type Output = T;
    fn push(&mut self, i: impl Into<Self::Output>) {
        self.push(i.into());
    }
}
struct Map<'a, T: OutputStream, F, O>(&'a mut T, F, PhantomData<O>);
impl<T, F, O> OutputStream for Map<'_, T, F, O>
where
    T: OutputStream,
    F: FnMut(O) -> T::Output,
{
    type Output = O;
    fn push(&mut self, i: impl Into<Self::Output>) {
        self.0.push(self.1(i.into()))
    }
}
struct Dummy<T>(PhantomData<T>);
impl<T> Dummy<T> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}
impl<T> OutputStream for Dummy<T> {
    type Output = T;
    fn push(&mut self, _i: impl Into<Self::Output>) {}
}
trait Push {
    fn push_spacer(&mut self);
    fn push_text_box(&mut self, global: &Static, element: &mut TextBox, state: State);
}
impl<T: OutputStream<Output = Element>> Push for T {
    fn push_spacer(&mut self) {
        self.push(Spacer::invisible())
    }
    fn push_text_box(&mut self, global: &Static, element: &mut TextBox, state: State) {
        let mut tb = std::mem::replace(element, TextBox::new(vec![], global.opts.hidpi_scale));
        element.indent = state.global_indent;

        if !tb.texts.is_empty() {
            let content = tb.texts.iter().any(|text| !text.text.is_empty());

            if content {
                tb.indent = state.global_indent;
                self.push(tb);
            }
        } else {
            element.is_checkbox = tb.is_checkbox;
        }
    }
}

pub struct AstOpts {
    pub anchorizer: Mutex<Anchorizer>,
    pub theme: Theme,
    pub hidpi_scale: f32,
    pub surface_format: TextureFormat,

    // needed for images
    pub color_scheme: Option<ResolvedTheme>,
    pub image_cache: ImageCache,
    pub window: Arc<Mutex<dyn WindowInteractor + Send>>,
}
impl AstOpts {
    fn native_color(&self, color: u32) -> [f32; 4] {
        native_color(color, &self.surface_format)
    }
}

pub struct Ast {
    pub opts: AstOpts,
    pub elements: Arc<Mutex<Vec<Element>>>,
}
impl Ast {
    pub fn new(opts: AstOpts, elements: Arc<Mutex<Vec<Element>>>) -> Self {
        Self { opts, elements }
    }
    pub fn interpret(&self, hir: Hir) {
        let nodes = hir.content();
        let root = nodes.first().unwrap().content.clone();
        let state =
            InheritedState::with_span_color(self.opts.native_color(self.opts.theme.code_color));

        let input = Input(&nodes);

        let global = Static {
            opts: &self.opts,
            input,
        };

        root.into_iter()
            .filter_map(|ton| {
                if let TextOrHirNode::Hir(node) = ton {
                    let mut out = vec![];
                    let mut tb = TextBox::new(vec![], self.opts.hidpi_scale);
                    let state = State::Borrowed(&state);
                    FlowProcess::process(
                        &global,
                        &mut tb,
                        state.borrow(),
                        global.input.get(node),
                        &mut out,
                    );
                    out.push_text_box(&global, &mut tb, state);
                    Some(out)
                } else {
                    None
                }
            })
            .for_each(|part| {
                self.elements.lock().extend(part);
                self.opts.window.lock().request_redraw();
            })
    }
}

struct Static<'a> {
    input: Input<'a>,
    opts: Opts<'a>,
}

enum State<'a> {
    Owned(InheritedState),
    Borrowed(&'a InheritedState),
}
impl Deref for State<'_> {
    type Target = InheritedState;
    fn deref(&self) -> &Self::Target {
        match self {
            State::Owned(ref inner) => inner,
            State::Borrowed(inner) => inner,
        }
    }
}
impl DerefMut for State<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.promote();
        match self {
            State::Owned(inner) => inner,
            _ => unreachable!(),
        }
    }
}
impl<'a> State<'a> {
    fn borrow(&'a self) -> Self {
        match self {
            State::Owned(ref inner) => State::Borrowed(inner),
            State::Borrowed(inner) => State::Borrowed(inner),
        }
    }
    /// Creates Owned variant
    fn promote(&mut self) {
        if let State::Borrowed(inner) = self {
            *self = State::Owned(inner.to_owned());
        }
    }
}
impl Clone for State<'_> {
    fn clone(&self) -> Self {
        match self {
            State::Owned(inner) => State::Owned(inner.clone()),
            State::Borrowed(inner) => State::Owned((*inner).clone()),
        }
    }
}

trait Process {
    type Context<'a>;
    fn process(
        global: &Static,
        element: Self::Context<'_>,
        state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    );
    fn process_content<'a>(
        _global: &Static,
        _element: Self::Context<'_>,
        _state: State,
        _input: impl IntoIterator<Item = &'a TextOrHirNode>,
        _output: &mut impl OutputStream<Output = Element>,
    ) {
        unimplemented!()
    }
    fn process_with<'a, I, N, T>(global: &Static, content: I, mut node_fn: N, mut text_fn: T)
    where
        I: IntoIterator<Item = &'a TextOrHirNode>,
        N: FnMut(&HirNode),
        T: FnMut(&String),
    {
        for ton in content {
            match ton {
                TextOrHirNode::Text(str) => text_fn(str),
                TextOrHirNode::Hir(node) => node_fn(global.input.get(*node)),
            }
        }
    }
    fn text(global: &Static, element: &mut TextBox, state: State, mut string: &str) {
        let text_native_color = global.opts.native_color(global.opts.theme.text_color);
        if string.trim().is_empty() {
            if state.text_options.pre_formatted {
                element.texts.push(Text::new(
                    "\n".to_string(),
                    global.opts.hidpi_scale,
                    text_native_color,
                ));
            }
            if let Some(last_text) = element.texts.last() {
                if let Some(last_char) = last_text.text.chars().last() {
                    if !last_char.is_whitespace() {
                        element.texts.push(Text::new(
                            " ".to_string(),
                            global.opts.hidpi_scale,
                            text_native_color,
                        ));
                    }
                }
            }
        } else {
            if element.texts.is_empty() && !state.text_options.pre_formatted {
                string = string.trim_start();
            }

            let mut text = Text::new(
                string.to_string(),
                global.opts.hidpi_scale,
                text_native_color,
            );

            if state.text_options.block_quote >= 1 {
                element.set_quote_block(state.text_options.block_quote as usize);
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
            if let Some(ref link) = state.text_options.link {
                text = text.with_link(link.to_string());
                text = text.with_color(global.opts.native_color(global.opts.theme.link_color));
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
                element.font_size = 12.;
            }
            element.texts.push(text);
        }
    }
}

struct FlowProcess;
impl Process for FlowProcess {
    type Context<'a> = &'a mut TextBox;
    fn process(
        global: &Static,
        element: Self::Context<'_>,
        mut state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        let attributes = &node.attributes;
        match node.tag {
            TagName::Paragraph => {
                state.set_align_from_attributes(attributes);
                element.set_align_or_default(state.text_options.align);

                FlowProcess::process_content(
                    global,
                    element,
                    state.borrow(),
                    &node.content,
                    output,
                );

                output.push_text_box(global, element, state);
                output.push_spacer();
            }
            TagName::Anchor => {
                for attr in attributes {
                    match attr {
                        Attr::Href(link) => {
                            let link = percent_decode_str(link)
                                .decode_utf8()
                                .expect("Should be valid when link is Utf8")
                                .into();
                            state.text_options.link = Some(link);
                        }
                        Attr::Anchor(a) => {
                            let a = percent_decode_str(a)
                                .decode_utf8()
                                .expect("Should be valid when link is Utf8")
                                .into_owned();
                            element.set_anchor(a.to_owned());
                        }
                        _ => {}
                    }
                }
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Div => {
                output.push_text_box(global, element, state.borrow());

                state.set_align_from_attributes(attributes);
                element.set_align_or_default(state.text_options.align);

                FlowProcess::process_content(
                    global,
                    element,
                    state.borrow(),
                    &node.content,
                    output,
                );
                output.push_text_box(global, element, state);
            }
            TagName::BlockQuote => {
                output.push_text_box(global, element, state.borrow());
                state.text_options.block_quote += 1;
                state.global_indent += DEFAULT_MARGIN / 2.;

                let indent = state.global_indent;

                FlowProcess::process_content(
                    global,
                    element,
                    state.borrow(),
                    &node.content,
                    output,
                );
                output.push_text_box(global, element, state);

                if indent == DEFAULT_MARGIN / 2. {
                    output.push_spacer();
                }
            }
            TagName::BoldOrStrong => {
                state.text_options.bold = true;
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Break => output.push_text_box(global, element, state),
            TagName::Code => {
                state.text_options.code = true;
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Details => {
                DetailsProcess::process(global, (), state, node, output);
            }
            TagName::Summary => tracing::warn!("Summary can only be in an Details element"),
            TagName::Section => {}
            TagName::EmphasisOrItalic => {
                state.text_options.italic = true;
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Header(header) => {
                output.push_text_box(global, element, state.borrow());
                output.push_spacer();

                state.set_align_from_attributes(attributes);
                element.set_align_or_default(state.text_options.align);

                state.text_options.bold = true;
                element.font_size *= header.size_multiplier();

                if header == HeaderType::H1 {
                    state.text_options.underline = true;
                }
                FlowProcess::process_content(
                    global,
                    element,
                    state.borrow(),
                    &node.content,
                    output,
                );

                let anchor = element.texts.iter().flat_map(|t| t.text.chars()).collect();
                let anchor = global.opts.anchorizer.lock().anchorize(anchor);
                element.set_anchor(format!("#{anchor}"));
                output.push_text_box(global, element, state);
                output.push_spacer();
            }
            TagName::HorizontalRuler => output.push(Spacer::visible()),
            TagName::Picture => PictureProcess::process(global, (), state, node, output),
            TagName::Source => tracing::warn!("Source tag can only be inside an Picture."),
            TagName::Image => ImageProcess::process(global, None, state, node, output),
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
                    element.set_checkbox(is_checked);
                }
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::ListItem => tracing::warn!("ListItem can only be in an List element"),
            TagName::OrderedList => {
                OrderedListProcess::process(global, element, state, node, output)
            }
            TagName::UnorderedList => {
                UnorderedListProcess::process(global, element, state, node, output)
            }
            TagName::PreformattedText => {
                output.push_text_box(global, element, state.borrow());
                let style = attributes
                    .iter()
                    .find_map(|attr| attr.to_style())
                    .unwrap_or_default();
                for style in style::Iter::new(&style) {
                    if let Style::BackgroundColor(color) = style {
                        let native_color = global.opts.native_color(color);
                        element.set_background_color(native_color);
                    }
                }
                state.text_options.pre_formatted = true;
                element.set_code_block(true);
                FlowProcess::process_content(
                    global,
                    element,
                    state.borrow(),
                    &node.content,
                    output,
                );

                output.push_text_box(global, element, state);
                output.push_spacer();
            }
            TagName::Small => {
                state.text_options.small = true;
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Span => {
                let style_str = attributes
                    .iter()
                    .find_map(|attr| attr.to_style())
                    .unwrap_or_default();
                for style in style::Iter::new(&style_str) {
                    match style {
                        Style::Color(color) => {
                            state.span.color = global.opts.native_color(color);
                        }
                        Style::FontWeight(weight) => state.span.weight = weight,
                        Style::FontStyle(style) => state.span.style = style,
                        Style::TextDecoration(decor) => state.span.decor = decor,
                        _ => {}
                    }
                }
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Strikethrough => {
                state.text_options.strike_through = true;
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Table => TableProcess::process(global, (), state, node, output),
            TagName::TableHead | TagName::TableBody => {
                tracing::warn!("TableHead and TableBody can only be in an Table element");
            }
            TagName::TableRow => tracing::warn!("TableRow can only be in an Table element"),
            TagName::TableDataCell => {
                tracing::warn!(
                    "TableDataCell can only be in an TableRow or an TableHeader element"
                );
            }
            TagName::TableHeader => {
                tracing::warn!("TableDataCell can only be in an TableRow element");
            }
            TagName::Underline => {
                state.text_options.underline = true;
                FlowProcess::process_content(global, element, state, &node.content, output);
            }
            TagName::Root => tracing::error!("Root element can't reach interpreter."),
        }
    }

    fn process_content<'a>(
        global: &Static,
        element: Self::Context<'_>,
        state: State,
        content: impl IntoIterator<Item = &'a TextOrHirNode>,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        for node in content {
            match node {
                TextOrHirNode::Text(string) => Self::text(global, element, state.borrow(), string),
                TextOrHirNode::Hir(node_index) => {
                    Self::process(
                        global,
                        element,
                        state.borrow(),
                        global.input.get(*node_index),
                        output,
                    );
                }
            }
        }
    }
}

struct DetailsProcess;
impl Process for DetailsProcess {
    type Context<'a> = ();
    fn process(
        global: &Static,
        _element: Self::Context<'_>,
        state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        let mut section = Section::bare(global.opts.hidpi_scale);
        *section.hidden.get_mut() = true;

        let mut content = node.content.iter();
        let mut tb = TextBox::new(vec![], global.opts.hidpi_scale);

        let Some(first) = node.content.first() else {
            return;
        };
        match first {
            TextOrHirNode::Hir(index) if global.input.get(*index).tag == TagName::Summary => {
                content.next();

                let summary = global.input.get(*index);

                FlowProcess::process_content(
                    global,
                    &mut tb,
                    state.borrow(),
                    &summary.content,
                    &mut Dummy::new(),
                );

                *section.summary = Some(Positioned::new(tb));
            }
            _ => {
                let mut tb = TextBox::new(vec![], global.opts.hidpi_scale);
                Self::text(global, &mut tb, state.borrow(), "Details");
                *section.summary = Some(Positioned::new(Element::TextBox(tb)))
            }
        }

        let mut section_content = vec![];
        let s = &mut section_content.map(Positioned::new);
        let mut tb = TextBox::new(vec![], global.opts.hidpi_scale);

        FlowProcess::process_content(global, &mut tb, state.borrow(), content, s);
        s.push_text_box(global, &mut tb, state);
        section.elements = section_content;
        output.push(section)
    }
}

struct OrderedListProcess;
impl Process for OrderedListProcess {
    type Context<'a> = &'a mut TextBox;
    fn process(
        global: &Static,
        element: Self::Context<'_>,
        mut state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        let mut index = 1;
        for attr in &node.attributes {
            if let Attr::Start(start) = attr {
                index = *start;
            }
        }
        output.push_text_box(global, element, state.borrow());
        state.global_indent += DEFAULT_MARGIN / 2.;

        Self::process_with(
            global,
            &node.content,
            |node| match node.tag {
                TagName::ListItem => {
                    ListItemProcess::process(
                        global,
                        (element, Some(index)),
                        state.borrow(),
                        node,
                        output,
                    );
                    index += 1;
                }
                _ => tracing::warn!("Only ListItems can be inside an List"),
            },
            |_| {},
        );
        if state.global_indent == DEFAULT_MARGIN / 2. {
            output.push_spacer();
        }
    }
}
struct UnorderedListProcess;
impl Process for UnorderedListProcess {
    type Context<'a> = &'a mut TextBox;
    fn process(
        global: &Static,
        element: Self::Context<'_>,
        mut state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        output.push_text_box(global, element, state.borrow());
        state.global_indent += DEFAULT_MARGIN / 2.;

        Self::process_with(
            global,
            &node.content,
            |node| match node.tag {
                TagName::ListItem => {
                    ListItemProcess::process(global, (element, None), state.borrow(), node, output);
                }
                _ => tracing::warn!("Only ListItems can be inside an List"),
            },
            |_| {},
        );
        if state.global_indent == DEFAULT_MARGIN / 2. {
            output.push_spacer();
        }
    }
}
struct ListItemProcess;
impl Process for ListItemProcess {
    type Context<'a> = (&'a mut TextBox, Option<usize>);
    fn process(
        global: &Static,
        (element, prefix): Self::Context<'_>,
        state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        let anchor = node.attributes.iter().find_map(|attr| attr.to_anchor());
        if let Some(anchor) = anchor {
            element.set_anchor(anchor)
        }
        let first_child_is_checkbox = if let Some(TextOrHirNode::Hir(node)) = node.content.first() {
            let node = global.input.get(*node);
            if node.tag == TagName::Input {
                node.attributes
                    .iter()
                    .any(|attr| matches!(attr, Attr::IsCheckbox))
            } else {
                false
            }
        } else {
            false
        };

        if !first_child_is_checkbox {
            let prefix = match prefix {
                Some(num) => format!("{num}. "),
                None => String::from("· "),
            };
            element.texts.push(
                Text::new(
                    prefix,
                    global.opts.hidpi_scale,
                    global.opts.native_color(global.opts.theme.text_color),
                )
                .make_bold(true),
            )
        }
        FlowProcess::process_content(global, element, state.borrow(), &node.content, output);
        output.push_text_box(global, element, state);
    }
}

struct ImageProcess;
impl ImageProcess {
    fn push_image_from_picture(
        opts: Opts,
        state: State,
        output: &mut impl OutputStream<Output = Element>,
        picture: Picture,
    ) {
        let align = picture.inner.align;
        let src = picture.resolve_src(opts.color_scheme).to_owned();
        let align = align.unwrap_or_default();
        let is_url = src.starts_with("http://") || src.starts_with("https://");
        let mut image = match opts.image_cache.lock().get(&src) {
            Some(image_data) if is_url => {
                Image::from_image_data(image_data.clone(), opts.hidpi_scale)
            }
            _ => {
                Image::from_src(src, opts.hidpi_scale, opts.window.lock().image_callback()).unwrap()
            }
        }
        .with_align(align);

        if let Some(ref link) = state.text_options.link {
            image.set_link(link.to_string())
        }
        if let Some(size) = picture.inner.size {
            image = image.with_size(size);
        }

        output.push(image);
        output.push_spacer()
    }
}
impl Process for ImageProcess {
    type Context<'a> = Option<Builder>;
    fn process(
        global: &Static,
        mut element: Self::Context<'_>,
        mut state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        if element.is_none() {
            element = Some(Picture::builder());
        }
        let mut builder = element.unwrap();

        state.set_align_from_attributes(&node.attributes);
        if let Some(align) = state.text_options.align {
            builder.set_align(align);
        }

        for attr in &node.attributes {
            match attr {
                Attr::Align(a) => builder.set_align(*a),
                Attr::Width(w) => builder.set_size(ImageSize::width(*w)),
                Attr::Height(h) => builder.set_size(ImageSize::height(*h)),
                Attr::Src(s) => builder.set_src(s.to_owned()),
                _ => {}
            }
        }

        match builder.try_finish() {
            Ok(pic) => Self::push_image_from_picture(global.opts, state, output, pic),
            Err(err) => tracing::warn!("Invalid <img>: {err}"),
        }
    }
}
struct SourceProcess;
impl Process for SourceProcess {
    type Context<'a> = &'a mut Builder;
    fn process(
        _global: &Static,
        element: Self::Context<'_>,
        _state: State,
        node: &HirNode,
        _output: &mut impl OutputStream<Output = Element>,
    ) {
        let mut media = None;
        let mut src_set = None;
        for attr in &node.attributes {
            match attr {
                Attr::Media(m) => media = Some(*m),
                Attr::SrcSet(s) => src_set = Some(s.to_owned()),
                _ => {}
            }
        }

        let Some((media, src_set)) = media.zip(src_set) else {
            tracing::info!("Skipping <source> tag. Missing either srcset or known media");
            return;
        };

        match media {
            PrefersColorScheme(ResolvedTheme::Dark) => element.set_dark_variant(src_set),
            PrefersColorScheme(ResolvedTheme::Light) => element.set_light_variant(src_set),
        }
    }
}
struct PictureProcess;
impl Process for PictureProcess {
    type Context<'a> = ();
    fn process(
        global: &Static,
        _element: Self::Context<'_>,
        mut state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        let mut builder = Picture::builder();

        let mut iter = node.content.iter().filter_map(|ton| match ton {
            TextOrHirNode::Text(_) => None,
            TextOrHirNode::Hir(node) => {
                let node = global.input.get(*node);
                match node.tag {
                    TagName::Image | TagName::Source => Some(node),
                    _ => None,
                }
            }
        });

        let Some(last) = iter.next_back() else {
            return;
        };

        for node in iter {
            SourceProcess::process(global, &mut builder, state.borrow(), node, output);
        }
        let attrs = &node.attributes;
        state.set_align_from_attributes(attrs);

        if let Some(ref align) = state.text_options.align {
            builder.set_align(*align);
        }

        ImageProcess::process(global, Some(builder), state, last, output)
    }
}

struct TableProcess;
impl Process for TableProcess {
    type Context<'a> = ();
    fn process(
        global: &Static,
        _element: Self::Context<'_>,
        state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        let mut table = Table::new();
        Self::process_with(
            global,
            &node.content,
            |node| {
                match node.tag {
                    TagName::TableHead | TagName::TableBody => {
                        TableHeadProcess::process(global, &mut table, state.borrow(), node, output);
                    }
                    TagName::TableRow => {
                        table.rows.push(vec![]);
                        TableRowProcess::process(global, &mut table, state.borrow(), node, output)
                    }
                    _ => tracing::warn!("Only TableHead, TableBody, TableRow and TableFoot can be inside an table, found: {:?}", node.tag),
                }
            },
            |_| {},
        );
        output.push_spacer();
        output.push(table);
        output.push_spacer();
    }
}

struct TableHeadProcess;
impl Process for TableHeadProcess {
    type Context<'a> = &'a mut Table;
    fn process(
        global: &Static,
        element: Self::Context<'_>,
        state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        Self::process_with(
            global,
            &node.content,
            |node| match node.tag {
                TagName::TableRow => {
                    element.rows.push(vec![]);
                    TableRowProcess::process(global, element, state.borrow(), node, output)
                }
                _ => tracing::warn!(
                    "Only TableRows can be inside an TableHead or TableBody, found {:?}",
                    node.tag
                ),
            },
            |_| {},
        );
    }
}

// https://html.spec.whatwg.org/multipage/tables.html#the-tr-element
struct TableRowProcess;
impl Process for TableRowProcess {
    type Context<'a> = &'a mut Table;
    fn process(
        global: &Static,
        element: Self::Context<'_>,
        state: State,
        node: &HirNode,
        output: &mut impl OutputStream<Output = Element>,
    ) {
        Self::process_with(
            global,
            &node.content,
            |node| {
                let mut state = state.clone();
                state.set_align_from_attributes(&node.attributes);
                match node.tag {
                    TagName::TableHeader => {
                        TableCellProcess::process(global, (element, true), state, node, output)
                    }
                    TagName::TableDataCell => {
                        TableCellProcess::process(global, (element, false), state, node, output)
                    }
                    _ => tracing::warn!(
                        "Only TableHeader and TableDataCell can be inside an TableRow, found: {:?}",
                        node.tag
                    ),
                }
            },
            |_| {},
        );
    }
}

// https://html.spec.whatwg.org/multipage/tables.html#the-th-element
// https://html.spec.whatwg.org/multipage/tables.html#the-td-element
struct TableCellProcess;
impl Process for TableCellProcess {
    /// (Table, IsHeader)
    type Context<'a> = (&'a mut Table, bool);
    fn process(
        global: &Static,
        (table, is_header): Self::Context<'_>,
        mut state: State,
        node: &HirNode,
        _output: &mut impl OutputStream<Output = Element>,
    ) {
        let row = table
            .rows
            .last_mut()
            .expect("There should be at least one row.");
        if is_header {
            state.text_options.bold = true;
        }

        let mut tb = TextBox::new(vec![], global.opts.hidpi_scale);
        tb.set_align_or_default(state.text_options.align);

        FlowProcess::process_content(
            global,
            &mut tb,
            state,
            &node.content,
            &mut Dummy::new(), // TODO allow anything inside tables not only text.
        );

        row.push(tb);
    }
}
