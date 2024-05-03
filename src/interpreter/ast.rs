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
use std::borrow::Cow;
use std::marker::PhantomData;
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
    fn set_align_from_attributes(&mut self, attributes: Attributes) {
        self.set_align(attributes.iter().find_map(|attr| attr.to_align()));
    }
}

type Content<'a> = &'a [TextOrHirNode];
type Attributes<'a> = &'a [Attr];
//pub type Output<'a> = &'a mut Vec<Element>;
pub type Input<'a> = &'a [HirNode];
type State<'a> = Cow<'a, InheritedState>;
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
    fn push_text_box(&mut self, text_box: &mut TextBox, opts: Opts, state: &State);
}
impl<T: OutputStream<Output = Element>> Push for T {
    fn push_spacer(&mut self) {
        self.push(Spacer::invisible())
    }
    fn push_text_box(&mut self, text_box: &mut TextBox, opts: Opts, state: &State) {
        let mut tb = std::mem::replace(text_box, TextBox::new(vec![], opts.hidpi_scale));
        text_box.indent = state.global_indent;

        if !tb.texts.is_empty() {
            let content = tb.texts.iter().any(|text| !text.text.is_empty());

            if content {
                tb.indent = state.global_indent;
                self.push(tb);
            }
        } else {
            text_box.is_checkbox = tb.is_checkbox;
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
}
impl Ast {
    pub fn new(opts: AstOpts) -> Self {
        Self { opts }
    }
    pub fn interpret(&self, hir: Hir) -> Vec<Element> {
        let nodes = hir.content();
        let root = nodes.first().unwrap().content.clone();
        let state = State::Owned(InheritedState::with_span_color(
            self.opts.native_color(self.opts.theme.code_color),
        ));
        root.into_iter()
            .filter_map(|ton| {
                if let TextOrHirNode::Hir(node) = ton {
                    let mut out = vec![];
                    let mut tb = TextBox::new(vec![], self.opts.hidpi_scale);
                    FlowProcess::process(
                        &nodes,
                        &mut out,
                        &self.opts,
                        &mut tb,
                        FlowProcess::get_node(&nodes, node),
                        state.clone(),
                    );
                    out.push_text_box(&mut tb, &self.opts, &state);
                    Some(out)
                } else {
                    None
                }
            })
            .flatten()
            .collect()
    }
}

macro_rules! out {
    () => {&mut impl OutputStream<Output=Element>};
}

trait Process {
    type Context<'a>;
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        state: State,
    );
    fn process_content<'a>(
        _input: Input,
        _output: out!(),
        _opts: Opts,
        _context: Self::Context<'_>,
        _content: impl IntoIterator<Item = &'a TextOrHirNode>,
        _state: State,
    ) {
        unimplemented!()
    }

    fn process_node<T, N>(input: Input, node: &HirNode, mut text_fn: T, mut node_fn: N)
    where
        T: FnMut(&String),
        N: FnMut(&HirNode),
    {
        node.content.iter().for_each(|node| match node {
            TextOrHirNode::Text(text) => text_fn(text),
            TextOrHirNode::Hir(node) => node_fn(Self::get_node(input, *node)),
        })
    }
    fn get_node(input: Input, index: usize) -> &HirNode {
        input.get(index).unwrap()
    }
    fn text(text_box: &mut TextBox, mut string: &str, opts: Opts, mut state: State) {
        let text_native_color = opts.native_color(opts.theme.text_color);
        if string == "\n" {
            if state.text_options.pre_formatted {
                text_box.texts.push(Text::new(
                    "\n".to_string(),
                    opts.hidpi_scale,
                    text_native_color,
                ));
            }
            if let Some(last_text) = text_box.texts.last() {
                if let Some(last_char) = last_text.text.chars().last() {
                    if !last_char.is_whitespace() {
                        text_box.texts.push(Text::new(
                            " ".to_string(),
                            opts.hidpi_scale,
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
                            opts.hidpi_scale,
                            text_native_color,
                        ));
                    }
                }
            }
        } else {
            if text_box.texts.is_empty() && !state.text_options.pre_formatted {
                string = string.trim_start();
            }

            let mut text = Text::new(string.to_string(), opts.hidpi_scale, text_native_color);

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
            if let Some(link) = state.to_mut().text_options.link.take() {
                text = text.with_link(link.to_string());
                text = text.with_color(opts.native_color(opts.theme.link_color));
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
}

struct FlowProcess;
impl Process for FlowProcess {
    type Context<'a> = &'a mut TextBox;
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        mut state: State,
    ) {
        let attributes = &node.attributes;
        match node.tag {
            TagName::Paragraph => {
                state.to_mut().set_align_from_attributes(attributes);
                context.set_align_or_default(state.text_options.align);

                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );

                output.push_text_box(context, opts, &state);
                output.push_spacer();
            }
            TagName::Anchor => {
                for attr in attributes {
                    match attr {
                        Attr::Href(link) => {
                            state.to_mut().text_options.link = Some(link.to_owned())
                        }
                        Attr::Anchor(a) => context.set_anchor(a.to_owned()),
                        _ => {}
                    }
                }
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Div => {
                output.push_text_box(context, opts, &state);

                state.to_mut().set_align_from_attributes(attributes);
                context.set_align_or_default(state.text_options.align);

                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
                output.push_text_box(context, opts, &state);
            }
            TagName::BlockQuote => {
                output.push_text_box(context, opts, &state);
                state.to_mut().text_options.block_quote += 1;
                state.to_mut().global_indent += DEFAULT_MARGIN / 2.;

                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );

                let indent = state.global_indent;

                output.push_text_box(context, opts, &state);

                if indent == DEFAULT_MARGIN / 2. {
                    output.push_spacer();
                }
            }
            TagName::BoldOrStrong => {
                state.to_mut().text_options.bold = true;
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Break => output.push_text_box(context, opts, &state),
            TagName::Code => {
                state.to_mut().text_options.code = true;
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Details => {
                DetailsProcess::process(input, output, opts, (), node, state);
            }
            TagName::Summary => tracing::warn!("Summary can only be in an Details element"),
            TagName::Section => {}
            TagName::EmphasisOrItalic => {
                state.to_mut().text_options.italic = true;
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Header(header) => {
                output.push_text_box(context, opts, &state);
                output.push_spacer();

                state.to_mut().set_align_from_attributes(attributes);
                context.set_align_or_default(state.text_options.align);

                state.to_mut().text_options.bold = true;
                context.font_size *= header.size_multiplier();

                if header == HeaderType::H1 {
                    state.to_mut().text_options.underline = true;
                }
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );

                let anchor = context.texts.iter().flat_map(|t| t.text.chars()).collect();
                let anchor = opts.anchorizer.lock().anchorize(anchor);
                context.set_anchor(format!("#{anchor}"));
                output.push_text_box(context, opts, &state);
                output.push_spacer();
            }
            TagName::HorizontalRuler => output.push(Spacer::visible()),
            TagName::Picture => PictureProcess::process(input, output, opts, (), node, state),
            TagName::Source => tracing::warn!("Source tag can only be inside an Picture."),
            TagName::Image => ImageProcess::process(input, output, opts, None, node, state),
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
                    context.set_checkbox(is_checked);
                }
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::ListItem => tracing::warn!("ListItem can only be in an List element"),
            TagName::OrderedList => {
                OrderedListProcess::process(input, output, opts, context, node, state.clone());
            }
            TagName::UnorderedList => {
                UnorderedListProcess::process(input, output, opts, context, node, state.clone());
            }
            TagName::PreformattedText => {
                output.push_text_box(context, opts, &state);
                let style = attributes
                    .iter()
                    .find_map(|attr| attr.to_style())
                    .unwrap_or_default();
                for style in style::Iter::new(&style) {
                    if let Style::BackgroundColor(color) = style {
                        let native_color = opts.native_color(color);
                        context.set_background_color(native_color);
                    }
                }
                state.to_mut().text_options.pre_formatted = true;
                context.set_code_block(true);
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );

                output.push_text_box(context, opts, &state);
                output.push_spacer();
            }
            TagName::Small => {
                state.to_mut().text_options.small = true;
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Span => {
                let style_str = attributes
                    .iter()
                    .find_map(|attr| attr.to_style())
                    .unwrap_or_default();
                for style in style::Iter::new(&style_str) {
                    match style {
                        Style::Color(color) => {
                            state.to_mut().span.color = opts.native_color(color);
                        }
                        Style::FontWeight(weight) => state.to_mut().span.weight = weight,
                        Style::FontStyle(style) => state.to_mut().span.style = style,
                        Style::TextDecoration(decor) => state.to_mut().span.decor = decor,
                        _ => {}
                    }
                }
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Strikethrough => {
                state.to_mut().text_options.strike_through = true;
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Table => {
                TableProcess::process(input, output, opts, (), node, state.clone());
            }
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
                state.to_mut().text_options.underline = true;
                FlowProcess::process_content(
                    input,
                    output,
                    opts,
                    context,
                    &node.content,
                    state.clone(),
                );
            }
            TagName::Root => tracing::error!("Root element can't reach interpreter."),
        }
    }

    fn process_content<'a>(
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        content: impl IntoIterator<Item = &'a TextOrHirNode>,
        state: State,
    ) {
        for node in content {
            match node {
                TextOrHirNode::Text(string) => {
                    Self::text(context, string.as_str(), opts, state.clone())
                }
                TextOrHirNode::Hir(node_index) => {
                    let node = Self::get_node(input, *node_index);
                    Self::process(input, output, opts, context, node, state.clone());
                }
            }
        }
    }
}

struct DetailsProcess;
impl Process for DetailsProcess {
    type Context<'a> = ();
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        _context: Self::Context<'_>,
        node: &HirNode,
        state: State,
    ) {
        let mut section = Section::bare(opts.hidpi_scale);
        *section.hidden.get_mut() = true;

        let mut content = node.content.iter();
        let mut tb = TextBox::new(vec![], opts.hidpi_scale);

        let Some(first) = node.content.first() else {
            return;
        };
        match first {
            TextOrHirNode::Hir(index) if Self::get_node(input, *index).tag == TagName::Summary => {
                content.next();

                let summary = Self::get_node(input, *index);

                FlowProcess::process_content(
                    input,
                    &mut Dummy::new(),
                    opts,
                    &mut tb,
                    summary.content.iter(),
                    state.clone(),
                );

                *section.summary = Some(Positioned::new(tb));
            }
            _ => {
                let mut tb = TextBox::new(vec![], opts.hidpi_scale);
                Self::text(&mut tb, "Details", opts, state.clone());
                *section.summary = Some(Positioned::new(Element::TextBox(tb)))
            }
        }

        let mut section_content = vec![];
        let s = &mut section_content.map(Positioned::new);
        let mut tb = TextBox::new(vec![], opts.hidpi_scale);

        FlowProcess::process_content(input, s, opts, &mut tb, content, state.clone());
        s.push_text_box(&mut tb, opts, &state);
        section.elements = section_content;
        output.push(section)
    }
}

struct OrderedListProcess;
impl Process for OrderedListProcess {
    type Context<'a> = &'a mut TextBox;
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        mut state: State,
    ) {
        let mut index = 1;
        for attr in &node.attributes {
            if let Attr::Start(start) = attr {
                index = *start;
            }
        }
        output.push_text_box(context, opts, &state);
        state.to_mut().global_indent += DEFAULT_MARGIN / 2.;

        Self::process_node(
            input,
            node,
            |_| {},
            |node| match node.tag {
                TagName::ListItem => {
                    ListItemProcess::process(
                        input,
                        output,
                        opts,
                        (context, Some(index)),
                        node,
                        state.clone(),
                    );
                    index += 1;
                }
                _ => tracing::warn!("Only ListItems can be inside an List"),
            },
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
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        mut state: State,
    ) {
        output.push_text_box(context, opts, &state);
        state.to_mut().global_indent += DEFAULT_MARGIN / 2.;

        Self::process_node(
            input,
            node,
            |_| {},
            |node| match node.tag {
                TagName::ListItem => {
                    ListItemProcess::process(
                        input,
                        output,
                        opts,
                        (context, None),
                        node,
                        state.clone(),
                    );
                }
                _ => tracing::warn!("Only ListItems can be inside an List"),
            },
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
        input: Input,
        output: out!(),
        opts: Opts,
        (context, list_prefix): Self::Context<'_>,
        node: &HirNode,
        state: State,
    ) {
        let anchor = node.attributes.iter().find_map(|attr| attr.to_anchor());

        let first_child_is_checkbox = if let Some(TextOrHirNode::Hir(node)) = node.content.first() {
            let node = Self::get_node(input, *node);
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
            let prefix = match list_prefix {
                Some(num) => format!("{num}. "),
                None => String::from("· "),
            };
            context.texts.push(
                Text::new(
                    prefix,
                    opts.hidpi_scale,
                    opts.native_color(opts.theme.text_color),
                )
                .make_bold(true),
            )
        }
        FlowProcess::process_content(input, output, opts, context, &node.content, state.clone());
        output.push_text_box(context, opts, &state)
    }
}

struct ImageProcess;
impl ImageProcess {
    fn push_image_from_picture(output: out!(), picture: Picture, opts: Opts, mut state: State) {
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

        if let Some(ref link) = state.to_mut().text_options.link {
            image.set_link(link.clone())
        }
        if let Some(size) = picture.inner.size {
            image = image.with_size(size);
        }

        output.push(image);
        //Self::push_spacer(output, );
    }
}
impl Process for ImageProcess {
    type Context<'a> = Option<Builder>;
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        mut context: Self::Context<'_>,
        node: &HirNode,
        mut state: State,
    ) {
        if context.is_none() {
            context = Some(Picture::builder());
        }
        let mut builder = context.unwrap();

        state.to_mut().set_align_from_attributes(&node.attributes);
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
            Ok(pic) => Self::push_image_from_picture(output, pic, opts, state.clone()), // TODO
            Err(err) => tracing::warn!("Invalid <img>: {err}"),
        }
    }
}
struct SourceProcess;
impl Process for SourceProcess {
    type Context<'a> = &'a mut Builder;
    fn process(
        _input: Input,
        _output: out!(),
        _opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        _state: State,
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
            PrefersColorScheme(ResolvedTheme::Dark) => context.set_dark_variant(src_set),
            PrefersColorScheme(ResolvedTheme::Light) => context.set_light_variant(src_set),
        }
    }
}
struct PictureProcess;
impl Process for PictureProcess {
    type Context<'a> = ();
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        _context: Self::Context<'_>,
        node: &HirNode,
        mut state: State,
    ) {
        let mut builder = Picture::builder();

        let mut iter = node.content.iter().filter_map(|ton| match ton {
            TextOrHirNode::Text(_) => None,
            TextOrHirNode::Hir(node) => {
                let node = Self::get_node(input, *node);
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
            SourceProcess::process(input, output, opts, &mut builder, node, state.clone());
        }

        state.to_mut().set_align_from_attributes(&node.attributes);
        if let Some(align) = state.text_options.align {
            builder.set_align(align);
        }

        ImageProcess::process(input, output, opts, Some(builder), last, state.clone())
    }
}

struct TableProcess;
impl Process for TableProcess {
    type Context<'a> = ();
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        state: State,
    ) {
        let mut table = Table::new();
        Self::process_node(
            input,
            node,
            |_| {},
            |node| {
                match node.tag {
                    TagName::TableHead | TagName::TableBody => {
                        TableHeadProcess::process(input, output, opts, &mut table, node, state.clone());
                    }
                    TagName::TableRow => {
                        table.rows.push(vec![]);
                        TableRowProcess::process(input, output, opts, &mut table, node, state.clone())
                    }
                    _ => tracing::warn!("Only TableHead, TableBody, TableRow and TableFoot can be inside an table, found: {:?}", node.tag),
                }
            },
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
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        state: State,
    ) {
        Self::process_node(
            input,
            node,
            |_| {},
            |node| match node.tag {
                TagName::TableRow => {
                    context.rows.push(vec![]);
                    TableRowProcess::process(input, output, opts, context, node, state.clone())
                }
                _ => tracing::warn!(
                    "Only TableRows can be inside an TableHead or TableBody, found {:?}",
                    node.tag
                ),
            },
        );
    }
}

// https://html.spec.whatwg.org/multipage/tables.html#the-tr-element
struct TableRowProcess;
impl Process for TableRowProcess {
    type Context<'a> = &'a mut Table;
    fn process(
        input: Input,
        output: out!(),
        opts: Opts,
        context: Self::Context<'_>,
        node: &HirNode,
        state: State,
    ) {
        Self::process_node(
            input,
            node,
            |_| {},
            |node| {
                let mut state = state.clone();
                state.to_mut().set_align_from_attributes(&node.attributes);
                match node.tag {
                    TagName::TableHeader => TableCellProcess::process(input, output, opts, (context, true), node, state),
                    TagName::TableDataCell => TableCellProcess::process(input, output, opts, (context, false), node, state),
                    _ => tracing::warn!("Only TableHead, TableBody, TableRow and TableFoot can be inside an table, found: {:?}", node.tag),
                }
            },
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
        input: Input,
        output: out!(),
        opts: Opts,
        (context, header): Self::Context<'_>,
        node: &HirNode,
        mut state: State,
    ) {
        let row = context
            .rows
            .last_mut()
            .expect("There should be at least one row.");
        // TODO allow anything inside tables not only text.
        if header {
            state.to_mut().text_options.bold = true;
        }

        let mut tb = TextBox::new(vec![], opts.hidpi_scale);
        tb.set_align_or_default(state.text_options.align);

        FlowProcess::process_content(
            input,
            &mut Dummy::new(),
            opts,
            &mut tb,
            &node.content,
            state.clone(),
        );

        row.push(tb);
    }
}
