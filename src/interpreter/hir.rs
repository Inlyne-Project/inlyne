use crate::interpreter::html::{self, Attr, TagName};
use crate::utils::markdown_to_html;
use anyhow::{bail, Context};
use html5ever::{
    buffer_queue::BufferQueue,
    tendril::{fmt, Tendril},
    tokenizer::{Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};
use smart_debug::SmartDebug;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    str::FromStr,
    sync::mpsc,
};
use syntect::highlighting::Theme;

type RcNode = Rc<RefCell<HirNode>>;
type WeakNode = Weak<RefCell<HirNode>>;

#[derive(Debug, Clone)]
pub enum TextOrHirNode {
    Text(String),
    Hir(RcNode),
}

#[derive(SmartDebug, Clone)]
pub struct HirNode {
    #[debug(skip)]
    pub parent: WeakNode,
    pub tag: TagName,
    pub attributes: Vec<Attr>,
    pub content: Vec<TextOrHirNode>,
}
pub fn unwrap_hir_node(node: RcNode) -> HirNode {
    Rc::try_unwrap(node).unwrap().into_inner()
}

#[derive(SmartDebug, Clone)]
pub struct Hir {
    root: RcNode,
    #[debug(skip)]
    current: RcNode,
    to_close: Vec<TagName>,
}
impl Hir {
    pub fn new() -> Self {
        let root = Rc::new(RefCell::new(HirNode {
            parent: Default::default(),
            tag: TagName::Root,
            attributes: vec![],
            content: vec![],
        }));
        Self {
            root: Rc::clone(&root),
            current: root,
            to_close: vec![TagName::Root],
        }
    }

    pub fn content(self) -> Vec<TextOrHirNode> {
        drop(self.current);
        unwrap_hir_node(self.root).content
    }

    pub fn transpile_md(self, receiver: mpsc::Receiver<String>, sender: mpsc::Sender<Hir>) {
        let mut input = BufferQueue::default();

        let mut tok = Tokenizer::new(self, TokenizerOpts::default());

        for md_string in receiver {
            tracing::debug!(
                "Received markdown for interpretation: {} bytes",
                md_string.len()
            );

            let html = markdown_to_html(&md_string, Theme::default());

            input.push_back(
                Tendril::from_str(&html)
                    .unwrap()
                    .try_reinterpret::<fmt::UTF8>()
                    .unwrap(),
            );

            let _ = tok.feed(&mut input);
            assert!(input.is_empty());
            tok.end();

            sender.send(tok.sink.clone()).unwrap();
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
        let attrs = html::attr::Iter::new(&tag.attrs).collect();

        let node = Rc::new(RefCell::new(HirNode {
            parent: Rc::downgrade(&self.current),
            tag: tag_name,
            attributes: attrs,
            content: vec![],
        }));

        self.current
            .borrow_mut()
            .content
            .push(TextOrHirNode::Hir(Rc::clone(&node)));

        if tag.self_closing || tag_name.is_void() {
            return;
        }

        self.current = node;
        self.to_close.push(tag_name);
    }
    fn process_end_tag(&mut self, tag: Tag) -> anyhow::Result<()> {
        let tag_name = match TagName::try_from(&tag.name) {
            Ok(name) => name,
            Err(name) => {
                bail!("Missing implementation for end tag: {name}");
            }
        };
        if tag_name.is_void() {
            return Ok(());
        }

        let to_close = self.to_close.pop().context("Expected closing tag")?;

        if tag_name != to_close {
            bail!("Expected closing {to_close:?} tag but found {tag_name:?}")
        }
        let parent = {
            self.current
                .borrow()
                .parent
                .upgrade()
                .context("Node has no parent")?
        };
        self.current = parent;

        Ok(())
    }
    fn on_text(&mut self, string: String) {
        self.current
            .borrow_mut()
            .content
            .push(TextOrHirNode::Text(string))
    }
    fn on_end(&mut self) {
        self.to_close.iter().skip(1).for_each(|unclosed_tag| {
            tracing::warn!("File contains unclosed html tag: {unclosed_tag:?}");
        })
    }
}

impl TokenSink for Hir {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => match tag.kind {
                TagKind::StartTag => self.process_start_tag(tag),
                TagKind::EndTag => {
                    let e = self.process_end_tag(tag);
                    if let Err(e) = e {
                        tracing::error!("{e}");
                    }
                }
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
