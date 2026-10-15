//! Transient, viewport-local keyboard labels. No document state is retained after cancellation.
use taffy::TaffyTree;

use crate::positioner::{Positioned, DEFAULT_MARGIN};
use crate::text::{Text, TextBox, TextBoxMeasure, TextSystem};
use crate::utils::{Point, Rect, Size};
use crate::Element;

const ALPHABET: &[u8] = b"asdfghjklqwertyuiopzxcvbnm";

#[derive(Debug)]
pub(crate) struct Hint {
    label: String,
    link: String,
    /// Computed from the full target set before hint mode starts; filtering never moves labels.
    pub label_rect: Rect,
    pub text_box: TextBox,
}

#[derive(Debug)]
pub(crate) struct LinkHints {
    hints: Vec<Hint>,
    prefix: String,
}

impl LinkHints {
    fn new(
        targets: Vec<(String, Rect, f32)>,
        text_system: &mut TextSystem,
        viewport: Size,
    ) -> Option<Self> {
        if targets.is_empty() {
            return None;
        }
        let labels = labels(targets.len());
        let mut hints = Vec::with_capacity(targets.len());
        for ((link, target, hidpi_scale), label) in targets.into_iter().zip(labels) {
            let text_box = TextBox {
                font_size: 13.,
                texts: vec![Text::new(label.clone(), hidpi_scale, [0., 0., 0., 1.]).make_bold(true)],
                hidpi_scale,
                ..Default::default()
            };
            let size = text_box.size(text_system, (f32::MAX, f32::INFINITY), 1.);
            let padding = 3. * hidpi_scale;
            let label_rect = place_label(
                target.pos,
                (size.0 + padding * 2., size.1 + padding * 2.),
                viewport,
                &hints,
                hidpi_scale,
            );
            hints.push(Hint {
                label,
                link,
                label_rect,
                text_box,
            });
        }
        Some(Self {
            hints,
            prefix: String::new(),
        })
    }

    pub fn matching(&self) -> impl Iterator<Item = &Hint> {
        self.hints
            .iter()
            .filter(|hint| hint.label.starts_with(&self.prefix))
    }

    /// Append a letter and return the destination when it completes a label.
    /// Invalid letters are ignored, allowing Backspace to recover without relabelling targets.
    pub fn push_letter(&mut self, letter: char) -> Option<String> {
        if !letter.is_ascii() || !ALPHABET.contains(&(letter as u8)) {
            return None;
        }
        self.prefix.push(letter);
        if self.matching().next().is_none() {
            self.prefix.pop();
            return None;
        }
        self.matching()
            .find(|hint| hint.label == self.prefix)
            .map(|hint| hint.link.clone())
    }

    pub fn backspace(&mut self) {
        self.prefix.pop();
    }

    pub fn collect(
        elements: &[Positioned<Element>],
        text_system: &mut TextSystem,
        taffy: &mut TaffyTree<TextBoxMeasure>,
        viewport: Rect,
        page_width: f32,
        zoom: f32,
    ) -> anyhow::Result<Option<Self>> {
        let mut targets = Vec::new();
        let centering = (viewport.size.0 - page_width).max(0.) / 2.;
        visit_leaves(elements, &mut |element, rect| {
            if visible_rect(rect, &viewport).is_none() {
                return Ok(());
            }
            let bounds = (
                (viewport.size.0 - rect.pos.0 - DEFAULT_MARGIN - centering).max(0.),
                f32::INFINITY,
            );
            match element {
                Element::TextBox(text_box) => {
                    let mut pos = rect.pos;
                    if text_box.is_checkbox.is_some() {
                        pos.0 += text_box.font_size * text_box.hidpi_scale * zoom * 0.75 * 1.5;
                    }
                    let bounds = (
                        (viewport.size.0 - pos.0 - DEFAULT_MARGIN - centering).max(0.),
                        bounds.1,
                    );
                    add_text_targets(
                        text_box,
                        text_system,
                        pos,
                        bounds,
                        zoom,
                        &viewport,
                        &mut targets,
                    );
                }
                Element::Table(table) => {
                    let layout = table.layout(text_system, taffy, bounds, zoom)?;
                    for (row, layouts) in table.rows.iter().zip(&layout.rows) {
                        for (text_box, node) in row.iter().zip(layouts) {
                            add_text_targets(
                                text_box,
                                text_system,
                                (rect.pos.0 + node.location.x, rect.pos.1 + node.location.y),
                                (node.size.width, f32::MAX),
                                zoom,
                                &viewport,
                                &mut targets,
                            );
                        }
                    }
                }
                Element::Image(image) => {
                    if let (Some(link), Some(visible)) =
                        (&image.is_link, visible_rect(rect, &viewport))
                    {
                        targets.push((link.clone(), visible, image.hidpi_scale));
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        Ok(Self::new(targets, text_system, viewport.size))
    }
}

fn add_text_targets(
    text_box: &TextBox,
    text_system: &mut TextSystem,
    pos: Point,
    bounds: Size,
    zoom: f32,
    viewport: &Rect,
    targets: &mut Vec<(String, Rect, f32)>,
) {
    let links = text_box.link_rects(
        &mut text_system.text_cache.lock(),
        &mut text_system.font_system.lock(),
        pos,
        bounds,
        zoom,
    );
    for (link, fragments) in links {
        if let Some(rect) = fragments
            .iter()
            .find_map(|rect| visible_rect(rect, viewport))
        {
            targets.push((link, rect, text_box.hidpi_scale));
        }
    }
}

/// Summaries toggle their section rather than following links in the mouse handler, so they
/// are deliberately excluded. Descend only into open sections, and recurse through all rows.
fn visit_leaves(
    elements: &[Positioned<Element>],
    visit: &mut impl FnMut(&Element, &Rect) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    for element in elements {
        match &element.inner {
            Element::Row(row) => visit_leaves(&row.elements, visit)?,
            Element::Section(section) if !*section.hidden.borrow() => {
                visit_leaves(&section.elements, visit)?
            }
            Element::Section(_) => {}
            leaf => {
                if let Some(rect) = &element.bounds {
                    visit(leaf, rect)?;
                }
            }
        }
    }
    Ok(())
}

/// Anchor labels to their links, nudging right past same-row neighbours when space permits.
/// Dense rows may overlap rather than disabling hints or moving them to unrelated rows.
fn place_label(pos: Point, size: Size, viewport: Size, placed: &[Hint], gap: f32) -> Rect {
    let max_x = (viewport.0 - size.0).max(0.);
    let mut rect = Rect::new(
        (
            pos.0.clamp(0., max_x),
            pos.1.clamp(0., (viewport.1 - size.1).max(0.)),
        ),
        size,
    );
    for hint in placed {
        let other = &hint.label_rect;
        if (rect.pos.1 - other.pos.1).abs() < gap
            && rect.pos.0 < other.max().0 + gap
            && rect.max().0 + gap > other.pos.0
            && other.max().0 + gap <= max_x
        {
            rect.pos.0 = other.max().0 + gap;
        }
    }
    rect
}

/// Clip both axes, reject empty/offscreen targets, then convert to viewport coordinates.
fn visible_rect(rect: &Rect, viewport: &Rect) -> Option<Rect> {
    let min = (
        rect.pos.0.max(viewport.pos.0),
        rect.pos.1.max(viewport.pos.1),
    );
    let max = (
        rect.max().0.min(viewport.max().0),
        rect.max().1.min(viewport.max().1),
    );
    (min.0 < max.0 && min.1 < max.1).then(|| {
        Rect::new(
            (min.0 - viewport.pos.0, min.1 - viewport.pos.1),
            (max.0 - min.0, max.1 - min.1),
        )
    })
}

/// Fixed-width base-N labels are prefix-free, including at alphabet-size boundaries.
fn labels(count: usize) -> Vec<String> {
    let mut width = 1;
    let mut highest = count.saturating_sub(1);
    while highest >= ALPHABET.len() {
        width += 1;
        highest /= ALPHABET.len();
    }
    (0..count)
        .map(|mut index| {
            let mut label = vec![ALPHABET[0]; width];
            for byte in label.iter_mut().rev() {
                *byte = ALPHABET[index % ALPHABET.len()];
                index /= ALPHABET.len();
            }
            String::from_utf8(label).expect("ASCII alphabet")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::positioner::{Row, Section};
    use std::cell::RefCell;
    use std::collections::HashSet;

    #[test]
    fn labels_are_unique_and_prefix_free_at_boundaries() {
        for count in [0, 1, 25, 26, 27, 676, 677, 2048] {
            let labels = labels(count);
            assert_eq!(labels.len(), count);
            assert_eq!(labels.iter().collect::<HashSet<_>>().len(), count);
            for (i, label) in labels.iter().enumerate() {
                assert!(label.bytes().all(|byte| ALPHABET.contains(&byte)));
                assert!(!labels
                    .iter()
                    .enumerate()
                    .any(|(j, other)| i != j && other.starts_with(label)));
            }
        }
    }

    fn hint(label: String, label_rect: Rect) -> Hint {
        Hint {
            label,
            link: String::new(),
            label_rect,
            text_box: TextBox::default(),
        }
    }

    #[test]
    fn filtering_and_backspace_preserve_labels_and_positions() {
        let mut hints = LinkHints {
            hints: labels(27)
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    let mut hint = hint(label, Rect::new((i as f32, 10.), (20., 20.)));
                    hint.link = i.to_string();
                    hint
                })
                .collect(),
            prefix: String::new(),
        };
        let positions: Vec<_> = hints
            .matching()
            .map(|hint| hint.label_rect.clone())
            .collect();
        assert_eq!(hints.push_letter('s'), None);
        assert_eq!(hints.matching().count(), 1);
        assert_eq!(hints.push_letter('m'), None); // Invalid prefixes do not hide every hint.
        assert_eq!(hints.prefix, "s");
        assert_eq!(hints.matching().next().unwrap().label_rect, positions[26]);
        hints.backspace();
        hints.backspace();
        assert_eq!(
            hints
                .matching()
                .map(|hint| hint.label_rect.clone())
                .collect::<Vec<_>>(),
            positions
        );
        hints.push_letter('s');
        assert_eq!(hints.push_letter('a'), Some("26".into()));
    }

    #[test]
    fn placement_nudges_neighbours_and_clamps_without_rejecting_dense_rows() {
        let viewport = (100., 100.);
        let first = place_label((10., 10.), (20., 20.), viewport, &[], 1.);
        assert_eq!(first.pos, (10., 10.));
        let placed = [hint("a".into(), first)];
        assert_eq!(
            place_label((10., 10.), (20., 20.), viewport, &placed, 1.).pos,
            (31., 10.)
        );
        // Consecutive lines must keep their anchors, even if the label backgrounds overlap.
        assert_eq!(
            place_label((10., 27.6), (20., 20.), viewport, &placed, 1.).pos,
            (10., 27.6)
        );
        assert_eq!(
            place_label((99., 99.), (20., 20.), viewport, &placed, 1.).pos,
            (80., 80.)
        );
        // At the right edge, keep a colliding label anchored rather than hiding the mode.
        let edge = [hint("a".into(), Rect::new((80., 80.), (20., 20.)))];
        assert_eq!(
            place_label((99., 99.), (20., 20.), viewport, &edge, 1.).pos,
            (80., 80.)
        );
    }

    #[test]
    fn visibility_clips_both_axes_and_partial_targets() {
        let viewport = Rect::new((0., 100.), (200., 100.));
        assert_eq!(
            visible_rect(&Rect::new((-5., 90.), (20., 20.)), &viewport),
            Some(Rect::new((0., 0.), (15., 10.)))
        );
        for rect in [
            Rect::new((0., 80.), (20., 20.)),
            Rect::new((200., 110.), (20., 20.)),
            Rect::new((0., 200.), (20., 20.)),
            Rect::new((0., 110.), (0., 20.)),
        ] {
            assert_eq!(visible_rect(&rect, &viewport), None);
        }
    }

    #[test]
    fn shaped_links_merge_styled_spans_and_keep_wrapped_visible_fragments() {
        use crate::text::{Text, TextCache};
        use crate::utils::Align;
        let mut fonts = glyphon::FontSystem::new();
        let mut cache = TextCache::new();
        let link =
            |text: &str| Text::new(text.into(), 1., [0., 0., 0., 1.]).with_link("#target".into());
        let mut text_box = TextBox::new(
            vec![link("a long wrapped "), link("styled link").make_bold(true)],
            1.,
        );
        let pos = (10., 20.);
        let bounds = (60., f32::INFINITY);
        let links = text_box.link_rects(&mut cache, &mut fonts, pos, bounds, 1.);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "#target");
        let rects = &links[0].1;
        assert!(rects.len() > 1);
        let viewport = Rect::new((0., rects[1].pos.1), (100., 100.));
        assert!(visible_rect(&rects[0], &viewport).is_none());
        assert!(rects
            .iter()
            .find_map(|rect| visible_rect(rect, &viewport))
            .is_some());

        // Rendered alignment and HiDPI/zoom must also apply to target geometry.
        text_box.align = Align::Right;
        let right = text_box.link_rects(&mut cache, &mut fonts, pos, (400., f32::INFINITY), 1.);
        assert!(right[0].1[0].pos.0 > pos.0);
        text_box.hidpi_scale = 2.;
        let scaled = text_box.link_rects(&mut cache, &mut fonts, pos, (400., f32::INFINITY), 1.5);
        assert_eq!(scaled[0].1[0].size.1, text_box.line_height(1.5));
    }

    #[test]
    fn walks_nested_rows_sections_and_tables_but_not_hidden_sections_or_summaries() {
        fn positioned(inner: Element) -> Positioned<Element> {
            Positioned {
                inner,
                bounds: Some(Rect::new((10., 10.), (100., 20.))),
            }
        }
        let table = || positioned(Element::Table(crate::Table::new()));
        let section = |hidden, elements| {
            positioned(Element::Section(Section {
                elements,
                summary: Box::new(Some(table())),
                hidden: RefCell::new(hidden),
                hidpi_scale: 1.,
            }))
        };
        let elements = vec![section(
            false,
            vec![positioned(Element::Row(Row {
                elements: vec![section(false, vec![table()]), section(true, vec![table()])],
                hidpi_scale: 1.,
            }))],
        )];
        let mut tables = 0;
        visit_leaves(&elements, &mut |element, _| {
            if matches!(element, Element::Table(_)) {
                tables += 1;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(tables, 1);
    }
}
