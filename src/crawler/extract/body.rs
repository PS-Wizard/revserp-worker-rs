use std::sync::LazyLock;

use scraper::{Html, Selector, node::Node};

use super::TextNormalizer;

static BODY_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("body").expect("hardcoded body selector must be valid"));

pub(super) fn extract_visible_text(document: &Html) -> String {
    let Some(body) = document.select(&BODY_SELECTOR).next() else {
        return String::new();
    };

    let mut normalizer = TextNormalizer::default();
    let mut nodes: Vec<_> = body.children().rev().map(|node| (node, false)).collect();

    while let Some((node, closing)) = nodes.pop() {
        if closing {
            normalizer.push(" ");
            continue;
        }

        match node.value() {
            Node::Element(element) if matches!(element.name(), "script" | "style" | "noscript") => {
                continue;
            }
            Node::Element(element) if is_text_boundary(element.name()) => {
                normalizer.push(" ");
                nodes.push((node, true));
            }
            Node::Text(text) => normalizer.push(text),
            _ => {}
        }

        nodes.extend(node.children().rev().map(|child| (child, false)));
    }

    normalizer.finish()
}

fn is_text_boundary(element: &str) -> bool {
    matches!(
        element,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "br"
            | "dd"
            | "details"
            | "dialog"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "li"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> String {
        extract_visible_text(&Html::parse_document(html))
    }

    #[test]
    fn extracts_normalized_body_text_in_document_order() {
        let text = extract(
            r#"
                <head><title>Ignored title</title></head>
                <body>
                    Hello <section> useful <strong>nested</strong> text </section> world
                </body>
            "#,
        );

        assert_eq!(text, "Hello useful nested text world");
    }

    #[test]
    fn separates_adjacent_structural_elements_without_changing_inline_text() {
        let text = extract(
            "<body><nav><ul><li><a>Home</a></li><li><a>Services</a></li></ul></nav><h1>Title</h1><p>Hello <strong>important</strong> world.</p><div>Next</div>tail<br>end</body>",
        );

        assert_eq!(
            text,
            "Home Services Title Hello important world. Next tail end"
        );
    }

    #[test]
    fn skips_script_style_and_noscript_subtrees() {
        let text = extract(
            r#"
                <body>
                    Kept
                    <script><span>script text</span></script>
                    <style>.hidden { display: none }</style>
                    <noscript>fallback text</noscript>
                    content
                </body>
            "#,
        );

        assert_eq!(text, "Kept content");
    }

    #[test]
    fn decodes_entities_and_ignores_comments() {
        let text = extract("<body>Tom &amp; Jerry<!-- hidden --> &lt;3</body>");

        assert_eq!(text, "Tom & Jerry <3");
    }

    #[test]
    fn returns_empty_text_when_body_is_absent() {
        let document = Html::parse_fragment("fragment text");

        assert!(extract_visible_text(&document).is_empty());
    }
}
