use std::collections::{HashMap, HashSet};

use crate::crawler::facts::CrawlFacts;

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const NO_INTERNAL_LINKS_OUT: IssueType =
    IssueType::new(Pillar::Seo, "internal_linking", "no_internal_links_out");
const LOW_INTERNAL_LINKS_OUT: IssueType =
    IssueType::new(Pillar::Seo, "internal_linking", "low_internal_links_out");
const ORPHAN_LIKE_PAGE: IssueType =
    IssueType::new(Pillar::Seo, "internal_linking", "orphan_like_page");
const LOW_INTERNAL_LINKS_IN: IssueType =
    IssueType::new(Pillar::Seo, "internal_linking", "low_internal_links_in");
const VERY_DEEP_PAGE: IssueType = IssueType::new(Pillar::Seo, "internal_linking", "very_deep_page");

const LOW_LINK_COUNT: usize = 2;
const VERY_DEEP_DEPTH: usize = 4;

pub(super) fn derive(facts: &CrawlFacts) -> Vec<DerivedIssue> {
    let mut inbound_sources_by_target: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut outbound_targets_by_source: HashMap<&str, HashSet<&str>> = HashMap::new();

    for link in &facts.links {
        let source = link.source_url.trim();
        let target = link.target_url.trim();
        if source.is_empty() || target.is_empty() || source == target {
            continue;
        }
        outbound_targets_by_source
            .entry(source)
            .or_default()
            .insert(target);
        inbound_sources_by_target
            .entry(target)
            .or_default()
            .insert(source);
    }

    facts
        .pages
        .iter()
        .filter(|page| page.is_healthy() && page.is_scoreable_content_type())
        .flat_map(|page| {
            let outbound_count = outbound_targets_by_source
                .get(page.url.trim())
                .map_or(0, HashSet::len);
            let inbound_count = inbound_sources_by_target
                .get(page.url.trim())
                .map_or(0, HashSet::len);
            let mut issues = Vec::new();

            if outbound_count == 0 {
                issues.push(DerivedIssue::new(
                    &page.url,
                    NO_INTERNAL_LINKS_OUT,
                    Severity::Medium,
                    "Page has no internal links out",
                    "Add internal links from this page to other pages on the site.".to_owned(),
                ));
            } else if outbound_count <= LOW_LINK_COUNT {
                issues.push(DerivedIssue::new(
                    &page.url,
                    LOW_INTERNAL_LINKS_OUT,
                    Severity::Low,
                    "Page has few internal links out",
                    format!("Page only links to {outbound_count} internal page(s)."),
                ));
            }

            if page.depth > 0 {
                if inbound_count == 0 {
                    issues.push(DerivedIssue::new(
                        &page.url,
                        ORPHAN_LIKE_PAGE,
                        Severity::High,
                        "Page appears orphan-like",
                        "Page has no discovered internal links pointing to it.".to_owned(),
                    ));
                } else if inbound_count <= LOW_LINK_COUNT {
                    issues.push(DerivedIssue::new(
                        &page.url,
                        LOW_INTERNAL_LINKS_IN,
                        Severity::Medium,
                        "Page has few internal links in",
                        format!("Page is linked from {inbound_count} internal page(s)."),
                    ));
                }
            }

            if page.depth >= VERY_DEEP_DEPTH {
                issues.push(DerivedIssue::new(
                    &page.url,
                    VERY_DEEP_PAGE,
                    Severity::Medium,
                    "Page is very deep in the crawl",
                    format!("Page was discovered at crawl depth {}.", page.depth),
                ));
            }

            issues
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawler::facts::{LinkFact, PageFact};

    fn page(url: &str, depth: usize) -> PageFact {
        PageFact {
            url: url.to_owned(),
            depth,
            status_code: 200,
            content_type: Some("text/html".to_owned()),
            ..PageFact::default()
        }
    }

    fn link(source_url: &str, target_url: &str) -> LinkFact {
        LinkFact {
            source_url: source_url.to_owned(),
            target_url: target_url.to_owned(),
            target_status: None,
        }
    }

    #[test]
    fn derives_outbound_inbound_orphan_and_depth_issues() {
        let root = "https://example.com/";
        let target = "https://example.com/target";
        let deep = "https://example.com/deep";
        let facts = CrawlFacts::new(
            vec![page(root, 0), page(target, 1), page(deep, 4)],
            vec![link(root, target), link(root, target), link(target, deep)],
        );

        let issues = derive(&facts);
        let ids_for = |url: &str| {
            issues
                .iter()
                .filter(|issue| issue.url == url)
                .map(|issue| issue.issue_type.id())
                .collect::<Vec<_>>()
        };

        assert_eq!(ids_for(root), vec!["low_internal_links_out"]);
        assert_eq!(
            ids_for(target),
            vec!["low_internal_links_out", "low_internal_links_in"]
        );
        assert_eq!(
            ids_for(deep),
            vec![
                "no_internal_links_out",
                "low_internal_links_in",
                "very_deep_page"
            ]
        );
    }

    #[test]
    fn ignores_self_links_and_non_scoreable_pages() {
        let root = "https://example.com/";
        let document = "https://example.com/file.pdf";
        let facts = CrawlFacts::new(
            vec![
                page(root, 0),
                PageFact {
                    url: document.to_owned(),
                    depth: 1,
                    status_code: 200,
                    content_type: Some("application/pdf".to_owned()),
                    ..PageFact::default()
                },
            ],
            vec![link(root, root)],
        );

        let issues = derive(&facts);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type.id(), "no_internal_links_out");
        assert_eq!(issues[0].url, root);
    }
}
