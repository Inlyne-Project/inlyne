# Link hints playground

Launch from the repository root with `--config assets/manual_test_data/link_hints.toml`.

1. Press **f** to label the links currently on screen.
2. Type a label to follow its link.
3. Press **Escape** to cancel, or **Backspace** to correct a partial label.

## Try these first

- [Jump down this document](#crowded-links) — an internal heading anchor.
- [Open a local Markdown file](link_hints_destination.md) — stays in Inlyne.
- [Inlyne on GitHub](https://github.com/Inlyne-Project/inlyne) — opens your browser.
- [Rust documentation](https://doc.rust-lang.org/) — another external link.

A [link with **bold text** and *italic text*](link_hints_destination.md) should still be reachable.

## Tables and images

| Kind | Target |
| --- | --- |
| Local file | [Open the destination](link_hints_destination.md) |
| Heading | [Jump to crowded links](#crowded-links) |

[![A linked image](link_hints.svg)](link_hints_destination.md)

<details>
<summary>A collapsed section</summary>

[This link must not get a hint until the section is expanded.](link_hints_destination.md)

</details>

<details>
<summary>Expand this section to test its link</summary>

[This link should get a hint after expansion.](link_hints_destination.md)

</details>

## Crowded links

These deliberately share a destination, but each link should be selectable.
With enough visible links, labels need more than one letter. Try a partial
label, Backspace, then Escape. Normal scrolling shortcuts must not run while
you are typing a label.

[01](link_hints_destination.md) · [02](link_hints_destination.md) · [03](link_hints_destination.md) · [04](link_hints_destination.md) · [05](link_hints_destination.md) · [06](link_hints_destination.md) · [07](link_hints_destination.md) · [08](link_hints_destination.md) · [09](link_hints_destination.md) · [10](link_hints_destination.md)

[11](link_hints_destination.md) · [12](link_hints_destination.md) · [13](link_hints_destination.md) · [14](link_hints_destination.md) · [15](link_hints_destination.md) · [16](link_hints_destination.md) · [17](link_hints_destination.md) · [18](link_hints_destination.md) · [19](link_hints_destination.md) · [20](link_hints_destination.md)

[21](link_hints_destination.md) · [22](link_hints_destination.md) · [23](link_hints_destination.md) · [24](link_hints_destination.md) · [25](link_hints_destination.md) · [26](link_hints_destination.md) · [27](link_hints_destination.md) · [28](link_hints_destination.md) · [29](link_hints_destination.md) · [30](link_hints_destination.md)

[31](link_hints_destination.md) · [32](link_hints_destination.md) · [33](link_hints_destination.md) · [34](link_hints_destination.md) · [35](link_hints_destination.md) · [36](link_hints_destination.md) · [37](link_hints_destination.md) · [38](link_hints_destination.md) · [39](link_hints_destination.md) · [40](link_hints_destination.md)

### Tightly packed targets

Try these at normal and reduced zoom. Labels nudge right where space permits;
very crowded rows may overlap.

[A](#link-hints-playground)[B](#crowded-links)[C](#tables-and-images)[D](#try-these-first)[E](#link-hints-playground)[F](#crowded-links)[G](#tables-and-images)[H](#try-these-first).

## Back to the top

[Return to the instructions](#link-hints-playground).
