use reqwest::Url;

use crate::crawler::extract::ExtractedPage;

const MIN_VISIBLE_TEXT_LENGTH: usize = 200;
const MIN_LINK_COUNT: usize = 3;
const MIN_SCRIPT_COUNT: usize = 5;
const MIN_INLINE_SCRIPT_LENGTH: usize = 1_000;
const MIN_SHELL_BODY_SIZE: usize = 10_000;
const RENDER_SCORE_THRESHOLD: usize = 5;

const JAVASCRIPT_SHELL_MARKERS: &[&str] = &[
    r#"id="__next""#,
    r#"id="root""#,
    r#"id="app""#,
    r#"id="__nuxt""#,
    r#"id="svelte""#,
    "data-reactroot",
    "window.__nuxt__",
    "__next_data__",
    "ng-app",
    "ng-version",
    "data-sveltekit",
    "astro-island",
];

const JAVASCRIPT_FRAMEWORK_MARKERS: &[&str] = &[
    "/_next/static/",
    "/_nuxt/",
    "/assets/index-",
    r#"type="module""#,
    "/build/",
    "data-reactroot",
    "ng-version",
    "data-sveltekit",
];

const JAVASCRIPT_REQUIRED_MESSAGES: &[&str] = &[
    "enable javascript",
    "enablejs",
    "requires javascript",
    "javascript is disabled",
    "you need to enable javascript",
    "please enable javascript",
];

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RenderDecision {
    pub(crate) needs_render: bool,
    pub(crate) score: usize,
    pub(crate) reasons: Vec<&'static str>,
}

pub(crate) fn needs_js_render(
    final_url: &Url,
    html: &[u8],
    page: &ExtractedPage,
) -> RenderDecision {
    let body = String::from_utf8_lossy(html).to_ascii_lowercase();
    let visible_text_is_sparse = page.visible_text.len() < MIN_VISIBLE_TEXT_LENGTH;
    let links_are_sparse = page.links.len() <= MIN_LINK_COUNT;
    let script_count = body.matches("<script").count();
    let inline_script_content_length = count_inline_script_content_length(&body);

    let mut score = 0;
    let mut reasons = Vec::new();

    if visible_text_is_sparse {
        score += 2;
        reasons.push("visible text is sparse");
    }

    if links_are_sparse {
        score += 1;
        reasons.push("link count is sparse");
    }

    if page.metadata.title.trim().is_empty() {
        score += 1;
        reasons.push("title is empty");
    }

    if script_count >= MIN_SCRIPT_COUNT && visible_text_is_sparse && links_are_sparse {
        score += 1;
        reasons.push("html contains many script tags");
    }

    if contains_any(&body, JAVASCRIPT_SHELL_MARKERS) {
        score += 3;
        reasons.push("html contains javascript app shell markers");
    }

    if contains_any(&body, JAVASCRIPT_FRAMEWORK_MARKERS) {
        score += 1;
        reasons.push("html contains javascript framework markers");
    }

    if inline_script_content_length >= MIN_INLINE_SCRIPT_LENGTH && visible_text_is_sparse {
        score += 3;
        reasons.push("html contains substantial inline script data");
    }

    let javascript_is_required = contains_any(&body, JAVASCRIPT_REQUIRED_MESSAGES)
        || final_url.as_str().to_ascii_lowercase().contains("enablejs");

    if javascript_is_required {
        reasons.push("html says javascript is required");
        return RenderDecision {
            needs_render: true,
            score,
            reasons,
        };
    }

    if html.len() > MIN_SHELL_BODY_SIZE && visible_text_is_sparse {
        score += 2;
        reasons.push("html body is large but extracted content is sparse");
    }

    RenderDecision {
        needs_render: score >= RENDER_SCORE_THRESHOLD,
        score,
        reasons,
    }
}

fn contains_any(body: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| body.contains(marker))
}

fn count_inline_script_content_length(body: &str) -> usize {
    let mut total = 0;
    let mut search_start = 0;

    while let Some(open_offset) = body[search_start..].find("<script") {
        let open_start = search_start + open_offset;
        let Some(tag_end_offset) = body[open_start..].find('>') else {
            break;
        };
        let content_start = open_start + tag_end_offset + 1;
        let Some(close_offset) = body[content_start..].find("</script>") else {
            break;
        };
        let content_end = content_start + close_offset;
        total += body[content_start..content_end].trim().len();
        search_start = content_end + "</script>".len();
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawler::extract::extract_page;

    fn decision(html: &str) -> RenderDecision {
        let url = Url::parse("https://example.com/").unwrap();
        let page = extract_page(html.as_bytes(), &url);
        needs_js_render(&url, html.as_bytes(), &page)
    }

    #[test]
    fn sparse_react_shell_needs_rendering() {
        let result = decision(
            r#"<html><head><title>Store</title></head><body><div id="root"></div></body></html>"#,
        );

        assert!(result.needs_render);
        assert!(
            result
                .reasons
                .contains(&"html contains javascript app shell markers")
        );
    }

    #[test]
    fn useful_static_html_does_not_need_rendering() {
        let text = "Useful static content. ".repeat(20);
        let html = format!(
            "<title>Page</title><body>{text}<a href=\"/a\">A</a><a href=\"/b\">B</a><a href=\"/c\">C</a><a href=\"/d\">D</a></body>"
        );

        assert!(!decision(&html).needs_render);
    }

    #[test]
    fn explicit_javascript_requirement_needs_rendering() {
        let result = decision("<p>Please enable JavaScript to continue</p>");

        assert!(result.needs_render);
        assert!(result.reasons.contains(&"html says javascript is required"));
    }

    #[test]
    fn sparse_static_page_does_not_need_rendering() {
        assert!(!decision("<title>Short note</title><p>Hello</p>").needs_render);
    }

    #[test]
    fn many_scripts_and_sparse_content_need_rendering() {
        let scripts = "<script></script>".repeat(MIN_SCRIPT_COUNT);
        let result = decision(&format!("<body>{scripts}</body>"));

        assert!(result.needs_render);
        assert!(result.reasons.contains(&"html contains many script tags"));
    }
}
