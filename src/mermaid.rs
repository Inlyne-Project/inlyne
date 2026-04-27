use std::sync::Mutex;

pub fn render_svg(source: &str) -> anyhow::Result<String> {
    match catch_renderer_unwind(|| render_svg_inner(source)) {
        Ok(result) => result,
        Err(payload) => {
            anyhow::bail!("Mermaid renderer panicked: {}", panic_message(&*payload));
        }
    }
}

fn render_svg_inner(source: &str) -> anyhow::Result<String> {
    let parsed = mermaid_rs_renderer::parse_mermaid(source)?;
    if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Flowchart
        && !has_explicit_flowchart_declaration(source)
    {
        anyhow::bail!("Mermaid flowchart is missing an explicit flowchart or graph declaration");
    }

    let theme = mermaid_rs_renderer::Theme::modern();
    let layout_config = mermaid_rs_renderer::LayoutConfig::default();
    let layout = mermaid_rs_renderer::compute_layout(&parsed.graph, &theme, &layout_config);
    Ok(mermaid_rs_renderer::render_svg(
        &layout,
        &theme,
        &layout_config,
    ))
}

fn catch_renderer_unwind<T>(render: impl FnOnce() -> T) -> std::thread::Result<T> {
    static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

    let _guard = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(render));
    std::panic::set_hook(previous_hook);
    result
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else {
        "unknown panic"
    }
}

fn has_explicit_flowchart_declaration(source: &str) -> bool {
    let Some(first_line) = first_diagram_line(source) else {
        return false;
    };
    let first_token = first_line
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ';')
        .next()
        .unwrap_or_default();

    first_token.eq_ignore_ascii_case("flowchart") || first_token.eq_ignore_ascii_case("graph")
}

fn first_diagram_line(source: &str) -> Option<&str> {
    let mut in_frontmatter = false;
    for line in source.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if line == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter || line.starts_with("%%") {
            continue;
        }
        return Some(line);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::render_svg;

    #[test]
    fn missing_flowchart_declaration_returns_error() {
        let err = render_svg("this is not a mermaid diagram")
            .expect_err("flowcharts should require an explicit declaration");
        assert!(
            !err.to_string().trim().is_empty(),
            "renderer error should be non-empty"
        );
    }

    #[test]
    fn supported_non_flowchart_diagram_renders() {
        let svg = render_svg(
            "\
sequenceDiagram
  Alice->>Bob: Hello
",
        )
        .expect("sequence diagrams should render through the renderer parser");
        assert!(svg.contains("<svg"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn renderer_layout_panic_returns_error() {
        let err = render_svg(
            "\
flowchart TD
    Member([Member<br/>uploader])
    Viewer([Viewer<br/>playback])

    subgraph Platform[\"Platform-owned orchestration (unchanged by provider choice)\"]
        direction LR
        P1[Processing placeholder<br/>+ predicted completion] --- P2[Reference / copy<br/>semantics] --- P3[Webhook normalization<br/>+ realtime fan-out] --- P4[S3 Glacier<br/>source archive] --- P5[Storage accounting<br/>+ entitlement gates]
    end

    subgraph Kaltura[\"KALTURA - single vendor\"]
        direction LR
        K1[Upload ingest<br/>chunked, resumable] --- K2[Transcode<br/>4 HLS renditions<br/>H.264 + AAC] --- K3[Thumbnail<br/>extraction] --- K4[Captions<br/>REACH ASR] --- K5[HLS serving<br/>signed URL + CDN]
    end

    Member -->|upload| Kaltura
    Kaltura -->|HLS playback| Viewer
    Platform -.-> Kaltura

    linkStyle 0,1,2,3,4,5,6,7 stroke:transparent,fill:none
",
        )
        .expect_err("renderer panics should become render errors");

        let err = err.to_string();
        assert!(err.contains("Mermaid renderer panicked"), "{err}");
        assert!(err.contains("subgraphs overlap"), "{err}");
    }
}
