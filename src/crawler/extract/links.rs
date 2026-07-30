use reqwest::Url;
use scraper::{Html, Selector};

use super::normalize_text;

static LINK_SELECTOR: std::sync::LazyLock<Selector> = std::sync::LazyLock::new(|| {
    Selector::parse("a[href]").expect("hardcoded link selector must be valid")
});

#[derive(Debug, Eq, PartialEq)]
pub struct ParsedLink {
    pub target_url: Url,
    pub anchor_text: String,
    pub internal: bool,
    pub nofollow: bool,
}

pub(super) fn extract_links(document: &Html, page_url: &Url) -> Vec<ParsedLink> {
    document
        .select(&LINK_SELECTOR)
        .filter_map(|anchor| {
            let target_url = normalize_url(anchor.value().attr("href")?, page_url)?;
            let rel = anchor.value().attr("rel").unwrap_or_default();

            Some(ParsedLink {
                internal: is_internal(page_url, &target_url),
                anchor_text: normalize_text(anchor.text()),
                target_url,
                nofollow: rel
                    .split_whitespace()
                    .any(|token| token.eq_ignore_ascii_case("nofollow")),
            })
        })
        .collect()
}

fn normalize_url(href: &str, page_url: &Url) -> Option<Url> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }

    if let Some(authority) = explicit_authority(href) {
        let host = authority.split(['/', '?', '#']).next().unwrap_or_default();
        if host.is_empty() {
            return None;
        }
    }

    let mut target_url = page_url.join(href).ok()?;
    if !matches!(target_url.scheme(), "http" | "https") || target_url.host_str().is_none() {
        return None;
    }

    target_url.set_fragment(None);
    if target_url.path().is_empty() {
        target_url.set_path("/");
    }
    Some(target_url)
}

fn explicit_authority(href: &str) -> Option<&str> {
    if let Some(authority) = href.strip_prefix("//") {
        return Some(authority);
    }

    for scheme in ["http:", "https:"] {
        if href.get(..scheme.len())?.eq_ignore_ascii_case(scheme) {
            return Some(href[scheme.len()..].strip_prefix("//").unwrap_or_default());
        }
    }

    None
}

fn is_internal(page_url: &Url, target_url: &Url) -> bool {
    match (page_url.host_str(), target_url.host_str()) {
        (Some(page_host), Some(target_host)) => {
            strip_one_www(page_host).eq_ignore_ascii_case(strip_one_www(target_host))
        }
        _ => false,
    }
}

fn strip_one_www(host: &str) -> &str {
    host.get(4..)
        .filter(|_| host[..4].eq_ignore_ascii_case("www."))
        .unwrap_or(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_url() -> Url {
        Url::parse("https://example.com/start").unwrap()
    }

    fn extract(html: &[u8], page_url: &Url) -> Vec<ParsedLink> {
        let body = String::from_utf8_lossy(html);
        let document = Html::parse_document(&body);
        extract_links(&document, page_url)
    }

    #[test]
    fn resolves_relative_links_and_collapses_descendant_text() {
        let links = extract(
            br#"<a href=" /docs/../about "> Hello <span>world</span>
                <em>wide</em> </a>"#,
            &page_url(),
        );

        assert_eq!(links[0].target_url.as_str(), "https://example.com/about");
        assert_eq!(links[0].anchor_text, "Hello world wide");
        assert!(links[0].internal);
    }

    #[test]
    fn marks_external_links_and_case_insensitive_nofollow_tokens() {
        let links = extract(
            br#"<a href="https://other.example/path" rel="external NOFOLLOW">Outside</a>"#,
            &page_url(),
        );

        assert!(!links[0].internal);
        assert!(links[0].nofollow);
    }

    #[test]
    fn ignores_www_and_ports_but_not_subdomains_for_internal_links() {
        let page = Url::parse("https://WWW.Example.com:8443/start").unwrap();
        let links = extract(
            br#"
                <a href="http://example.com:1/apex">apex</a>
                <a href="https://www.example.com:2/www">www</a>
                <a href="https://sub.example.com/path">subdomain</a>
            "#,
            &page,
        );

        assert!(links[0].internal);
        assert!(links[1].internal);
        assert!(!links[2].internal);
    }

    #[test]
    fn removes_fragments_without_changing_document_order() {
        let links = extract(
            br#"
                <a href="/first#one">first</a>
                <a href="//EXAMPLE.com/second#two">second</a>
            "#,
            &page_url(),
        );

        assert_eq!(
            links
                .iter()
                .map(|link| link.target_url.as_str())
                .collect::<Vec<_>>(),
            ["https://example.com/first", "https://example.com/second"]
        );
    }

    #[test]
    fn skips_empty_unsupported_and_invalid_targets() {
        let links = extract(
            br#"
                <a href=""></a>
                <a href="   "></a>
                <a href="mailto:test@example.com"></a>
                <a href="javascript:void(0)"></a>
                <a href="http:///missing-host"></a>
                <a href="https:///missing-host"></a>
                <a href="https:/missing-host"></a>
                <a href="HTTPS:///missing-host"></a>
                <a href="http://[::1"></a>
            "#,
            &page_url(),
        );

        assert!(links.is_empty());
    }

    #[test]
    fn accepts_valid_https_links() {
        let links = extract(
            br#"<a href="HTTPS://example.com/path">valid</a>"#,
            &page_url(),
        );

        assert_eq!(links[0].target_url.as_str(), "https://example.com/path");
    }

    #[test]
    fn preserves_duplicate_links() {
        let links = extract(
            br#"
                <a href="/same">one</a>
                <a href="/same">two</a>
            "#,
            &page_url(),
        );

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_url.as_str(), links[1].target_url.as_str());
    }
}
