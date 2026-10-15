//! Transient, viewport-local keyboard labels. No document state is retained after cancellation.
use taffy::TaffyTree;

use crate::positioner::{Positioned, DEFAULT_MARGIN};
use crate::text::{Text, TextBox, TextBoxMeasure, TextSystem};
use crate::utils::{Rect, Size};
use crate::Element;

const ALPHABET: &[u8] = b"asdfghjklqwertyuiopzxcvbnm";

#[derive(Debug)]
pub(crate) struct Hint {
    pub label: String,
    pub link: String,
    /// Visible part of the target in viewport coordinates.
    pub rect: Rect,
    /// Computed from the full target set before hint mode starts; filtering never moves labels.
    pub label_rect: Rect,
    pub text_box: TextBox,
}

#[derive(Debug)]
pub(crate) struct LinkHints {
    pub hints: Vec<Hint>,
    pub prefix: String,
}

impl LinkHints {
    fn new(targets: Vec<(String, Rect, f32)>) -> Option<Self> {
        if targets.is_empty() {
            return None;
        }
        let labels = labels(targets.len());
        Some(Self {
            hints: targets
                .into_iter()
                .zip(labels)
                .map(|((link, rect, hidpi_scale), label)| {
                    let text =
                        Text::new(label.clone(), hidpi_scale, [0., 0., 0., 1.]).make_bold(true);
                    Hint {
                        label,
                        link,
                        rect,
                        label_rect: Rect::new((0., 0.), (0., 0.)),
                        text_box: TextBox {
                            font_size: 13.,
                            texts: vec![text],
                            hidpi_scale,
                            ..Default::default()
                        },
                    }
                })
                .collect(),
            prefix: String::new(),
        })
    }

    pub fn matching(&self) -> impl Iterator<Item = &Hint> {
        self.hints
            .iter()
            .filter(|hint| hint.label.starts_with(&self.prefix))
    }

    /// Invalid letters are ignored, allowing Backspace to recover without relabelling targets.
    pub fn push(&mut self, letter: char) -> Option<String> {
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
        let Some(mut hints) = Self::new(targets) else {
            return Ok(None);
        };
        hints.place(text_system, viewport.size)?;
        Ok(Some(hints))
    }

    fn place(&mut self, text_system: &mut TextSystem, viewport: Size) -> anyhow::Result<()> {
        let mut gap = 0_f32;
        let targets: Vec<_> = self
            .hints
            .iter()
            .map(|hint| {
                let padding = 3. * hint.text_box.hidpi_scale;
                gap = gap.max(hint.text_box.hidpi_scale);
                let size = hint
                    .text_box
                    .size(text_system, (f32::MAX, f32::INFINITY), 1.);
                (
                    hint.rect.clone(),
                    (size.0 + padding * 2., size.1 + padding * 2.),
                )
            })
            .collect();
        let placements = place_labels(&targets, viewport, gap).ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot fit {} link hints without overlap; zoom in or resize the window",
                self.hints.len()
            )
        })?;
        for (hint, rect) in self.hints.iter_mut().zip(placements) {
            hint.label_rect = rect;
        }
        Ok(())
    }
}

fn add_text_targets(
    text_box: &TextBox,
    text_system: &mut TextSystem,
    pos: (f32, f32),
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

/// Greedily place full-sized labels as close to their targets as space permits. For each
/// candidate row, occupied rectangles define blocked horizontal intervals; choose the closest
/// free interval. Rows at obstacle edges plus viewport edges cover all possible free regions.
/// The full target set is placed once, not only the labels matching the current prefix.
/// Failure is explicit: callers must not enter hint mode with missing or overlapping labels.
fn place_labels(targets: &[(Rect, Size)], viewport: Size, gap: f32) -> Option<Vec<Rect>> {
    if targets
        .iter()
        .any(|(_, size)| size.0 > viewport.0 || size.1 > viewport.1)
        || targets
            .iter()
            .map(|(_, size)| f64::from(size.0) * f64::from(size.1))
            .sum::<f64>()
            > f64::from(viewport.0) * f64::from(viewport.1)
    {
        return None;
    }
    let mut placed: Vec<Rect> = Vec::with_capacity(targets.len());
    for (target, size) in targets {
        let preferred = (
            target.pos.0.clamp(0., viewport.0 - size.0),
            target.pos.1.clamp(0., viewport.1 - size.1),
        );
        let mut rows = vec![preferred.1, 0., viewport.1 - size.1];
        for rect in &placed {
            rows.push((rect.pos.1 - size.1 - gap).clamp(0., viewport.1 - size.1));
            rows.push((rect.max().1 + gap).clamp(0., viewport.1 - size.1));
        }
        rows.sort_unstable_by(|a, b| {
            (a - preferred.1)
                .abs()
                .total_cmp(&(b - preferred.1).abs())
                .then(a.total_cmp(b))
        });
        rows.dedup();
        let mut best: Option<(f32, Rect)> = None;
        for y in rows {
            let dy_squared = (y - preferred.1).powi(2);
            if best
                .as_ref()
                .is_some_and(|(distance, _)| dy_squared > *distance)
            {
                break;
            }
            let mut blocked: Vec<_> = placed
                .iter()
                .filter(|rect| y < rect.max().1 + gap && y + size.1 + gap > rect.pos.1)
                .map(|rect| {
                    (
                        (rect.pos.0 - gap).max(0.),
                        (rect.max().0 + gap).min(viewport.0),
                    )
                })
                .collect();
            blocked.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut left = 0_f32;
            for (start, end) in blocked
                .into_iter()
                .chain(std::iter::once((viewport.0, viewport.0)))
            {
                if start - left >= size.0 {
                    let x = preferred.0.clamp(left, start - size.0);
                    let distance = (x - preferred.0).powi(2) + dy_squared;
                    if best
                        .as_ref()
                        .is_none_or(|(previous, _)| distance < *previous)
                    {
                        best = Some((distance, Rect::new((x, y), *size)));
                    }
                }
                left = left.max(end);
            }
        }
        placed.push(best?.1);
    }
    Some(placed)
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

    #[test]
    fn prefix_filter_and_backspace_keep_labels_stable() {
        let mut hints = LinkHints::new(
            (0..27)
                .map(|i| (i.to_string(), Rect::new((0., 0.), (10., 10.)), 1.))
                .collect(),
        )
        .unwrap();
        assert_eq!(hints.push('s'), None);
        assert_eq!(hints.matching().count(), 1);
        assert_eq!(hints.push('m'), None); // No such prefix, preserve "s".
        assert_eq!(hints.prefix, "s");
        hints.backspace();
        hints.backspace();
        assert_eq!(hints.matching().count(), 27);
        assert_eq!(hints.push('s'), None);
        assert_eq!(hints.push('a'), Some("26".into()));
    }

    fn assert_usable_placements(placements: &[Rect], viewport: Size) {
        for (i, rect) in placements.iter().enumerate() {
            assert!(rect.pos.0 >= 0. && rect.pos.1 >= 0.);
            assert!(rect.max().0 <= viewport.0 + 0.001 && rect.max().1 <= viewport.1 + 0.001);
            for other in &placements[..i] {
                assert!(
                    rect.max().0 <= other.pos.0
                        || other.max().0 <= rect.pos.0
                        || rect.max().1 <= other.pos.1
                        || other.max().1 <= rect.pos.1,
                    "overlapping labels: {rect:?} and {other:?}"
                );
            }
        }
    }

    #[test]
    fn placement_separates_tightly_spaced_links_at_normal_and_reduced_zoom() {
        let viewport = (400., 240.);
        for zoom in [1., 0.5, 0.25] {
            // Label font stays readable at reduced document zoom, independent of tiny targets.
            let targets: Vec<_> = (0..40)
                .map(|i| {
                    (
                        Rect::new(
                            (
                                20. + (i % 10) as f32 * 18. * zoom,
                                20. + (i / 10) as f32 * 18. * zoom,
                            ),
                            (5. * zoom, 12. * zoom),
                        ),
                        (22., 20.),
                    )
                })
                .collect();
            let placements = place_labels(&targets, viewport, 1.).unwrap();
            assert_usable_placements(&placements, viewport);
            assert_eq!(placements, place_labels(&targets, viewport, 1.).unwrap());
            assert!(placements
                .iter()
                .zip(&targets)
                .all(
                    |(rect, (target, _))| (rect.pos.0 - target.pos.0).abs() < 200.
                        && (rect.pos.1 - target.pos.1).abs() < 200.
                ));
        }
    }

    #[test]
    fn placement_preserves_clear_targets_and_keeps_edge_labels_inside() {
        let viewport = (100., 100.);
        let targets = vec![
            (Rect::new((5., 10.), (5., 5.)), (20., 15.)),
            (Rect::new((50., 50.), (5., 5.)), (20., 15.)),
            (Rect::new((99., 99.), (1., 1.)), (20., 15.)),
        ];
        let placements = place_labels(&targets, viewport, 1.).unwrap();
        assert_eq!(placements[0].pos, targets[0].0.pos);
        assert_eq!(placements[1].pos, targets[1].0.pos);
        assert_eq!(placements[2].pos, (80., 85.));
        assert_usable_placements(&placements, viewport);
    }

    #[test]
    fn filtering_and_backspace_preserve_precomputed_placements() {
        let mut hints = LinkHints::new(
            (0..27)
                .map(|i| (i.to_string(), Rect::new((i as f32, 10.), (1., 5.)), 1.))
                .collect(),
        )
        .unwrap();
        let inputs: Vec<_> = hints
            .hints
            .iter()
            .map(|hint| (hint.rect.clone(), (20., 20.)))
            .collect();
        let placements = place_labels(&inputs, (240., 180.), 1.).unwrap();
        for (hint, rect) in hints.hints.iter_mut().zip(&placements) {
            hint.label_rect = rect.clone();
        }
        hints.push('s');
        assert_eq!(hints.matching().next().unwrap().label_rect, placements[26]);
        hints.backspace();
        assert_eq!(
            hints
                .matching()
                .map(|hint| hint.label_rect.clone())
                .collect::<Vec<_>>(),
            placements
        );
    }

    #[test]
    fn impossible_layout_fails_instead_of_omitting_or_overlapping_labels() {
        let target = Rect::new((0., 0.), (1., 1.));
        assert!(place_labels(&[(target.clone(), (101., 20.))], (100., 100.), 1.).is_none());
        assert!(place_labels(&vec![(target, (20., 20.)); 26], (100., 100.), 1.).is_none());
        assert_eq!(place_labels(&[], (100., 100.), 1.), Some(Vec::new()));
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
