use crate::interpreter::html::{self, Attr, TagName};
use html5ever::tokenizer::{Tag, TagKind, Token, TokenSink, TokenSinkResult};
use smart_debug::SmartDebug;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum TextOrHirNode {
    Text(String),
    Hir(usize),
}

#[derive(SmartDebug, Clone)]
pub struct HirNode {
    pub tag: TagName,
    pub attributes: Vec<Attr>,
    pub content: Vec<TextOrHirNode>,
}
impl HirNode {
    const fn new(tag: TagName, attributes: Vec<Attr>) -> Self {
        Self {
            tag,
            attributes,
            content: vec![],
        }
    }
}

#[derive(SmartDebug, Clone)]
pub struct Hir {
    nodes: Vec<HirNode>,
    #[debug(skip)]
    parents: Vec<usize>,
    to_close: Vec<TagName>,
}
impl Hir {
    pub fn new() -> Self {
        let root = HirNode {
            tag: TagName::Root,
            attributes: vec![],
            content: vec![],
        };
        Self {
            nodes: vec![root],
            parents: vec![0],
            to_close: vec![TagName::Root],
        }
    }

    pub fn content(self) -> Vec<HirNode> {
        self.nodes
    }

    fn current_node(&mut self) -> &mut HirNode {
        self.nodes
            .get_mut(
                *self
                    .parents
                    .last()
                    .expect("There should be at least one parent"),
            )
            .expect("Any parent should be in nodes")
    }

    fn process_start_tag(&mut self, tag: Tag) {
        let tag_name = match TagName::try_from(&tag.name) {
            Ok(name) => name,
            Err(name) => {
                tracing::info!("Missing implementation for tag: {name}");
                return;
            }
        };
        let attrs = html::attr::Iter::new(&tag.attrs).collect();

        let index = self.nodes.len();
        self.current_node().content.push(TextOrHirNode::Hir(index));

        self.nodes.push(HirNode::new(tag_name, attrs));

        if tag.self_closing || tag_name.is_void() {
            return;
        }
        self.parents.push(self.nodes.len() - 1);
        self.to_close.push(tag_name);
    }
    fn process_end_tag(&mut self, tag: Tag) {
        let tag_name = match TagName::try_from(&tag.name) {
            Ok(name) => name,
            Err(_) => return,
        };
        if tag_name.is_void() {
            return;
        }

        let Some(to_close) = self.to_close.pop() else {
            return;
        };
        if to_close == TagName::Root {
            tracing::warn!("Found unexpected/unopened closing {tag_name:?}");
            return;
        }
        if tag_name != to_close {
            tracing::warn!("Expected closing {to_close:?} tag but found {tag_name:?}")
        }
        self.parents.pop();
    }
    fn on_text(&mut self, string: String) {
        let current_node = self.current_node();

        if matches!(
            current_node.tag,
            TagName::PreformattedText | TagName::Details
        ) && current_node.content.is_empty()
            && string.trim().is_empty()
        {
            return;
        }

        current_node.content.push(TextOrHirNode::Text(string));
    }
    fn on_end(&mut self) {
        self.to_close.iter().skip(1).for_each(|unclosed_tag| {
            tracing::warn!("File contains unclosed html tag: {unclosed_tag:?}");
        });
    }
}

impl TokenSink for Hir {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => match tag.kind {
                TagKind::StartTag => self.process_start_tag(tag),
                TagKind::EndTag => self.process_end_tag(tag),
            },
            Token::CharacterTokens(str) => self.on_text(str.to_string()),
            Token::EOFToken => self.on_end(),
            Token::ParseError(err) => tracing::warn!("HTML parser emitted error: {err}"),
            Token::DoctypeToken(_) | Token::CommentToken(_) | Token::NullCharacterToken => {}
        }
        TokenSinkResult::Continue
    }
}
impl Default for Hir {
    fn default() -> Self {
        Self::new()
    }
}
impl Display for Hir {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fn fmt_inner(
            f: &mut Formatter<'_>,
            hir: &Hir,
            current: usize,
            mut indent: usize,
        ) -> std::fmt::Result {
            let node = hir.nodes.get(current).ok_or(std::fmt::Error)?;

            writeln!(f, "{:>indent$}{:?}:", "", node.tag)?;
            indent += 2;
            for ton in &node.content {
                match ton {
                    TextOrHirNode::Text(str) => writeln!(f, "{:>indent$}{str:?}", "")?,
                    TextOrHirNode::Hir(node) => fmt_inner(f, hir, *node, indent)?,
                }
            }
            Ok(())
        }
        fmt_inner(f, self, 0, 0)
    }
}
