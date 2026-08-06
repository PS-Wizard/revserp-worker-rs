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
const INTERNAL_LINKS_TO_BROKEN_PAGES: IssueType = IssueType::new(
    Pillar::Seo,
    "internal_linking",
    "internal_links_to_broken_pages",
);

const LOW_LINK_COUNT: usize = 2;
const VERY_DEEP_DEPTH: usize = 4;

pub(super) fn derive(facts: &CrawlFacts) -> Vec<DerivedIssue> {
    let mut inbound_sources_by_target: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut outbound_targets_by_source: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut broken_targets_by_source: HashMap<&str, HashSet<&str>> = HashMap::new();

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
        if link.target_status.is_some_and(|status| status >= 400) {
            broken_targets_by_source
                .entry(source)
                .or_default()
                .insert(target);
        }
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

            if let Some(broken_targets) = broken_targets_by_source.get(page.url.trim()) {
                let mut targets = broken_targets.iter().copied().collect::<Vec<_>>();
                targets.sort_unstable();
                let mut shown_targets = targets
                    .iter()
                    .take(3)
                    .map(|target| (*target).to_owned())
                    .collect::<Vec<_>>();
                if targets.len() > 3 {
                    shown_targets.push(format!("and {} more", targets.len() - 3));
                }
                issues.push(DerivedIssue::new(
                    &page.url,
                    INTERNAL_LINKS_TO_BROKEN_PAGES,
                    Severity::High,
                    "Page links to broken internal targets",
                    format!(
                        "Page links to {} broken internal target(s): {}.",
                        targets.len(),
                        shown_targets.join(", "),
                    ),
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

    #[test]
    fn derives_broken_target_issue_with_unique_sorted_details() {
        let source = "https://example.com/source";
        let first = "https://example.com/first";
        let second = "https://example.com/second";
        let third = "https://example.com/third";
        let fourth = "https://example.com/fourth";
        let mut broken_first = link(source, first);
        broken_first.target_status = Some(404);
        let mut broken_second = link(source, second);
        broken_second.target_status = Some(500);
        let mut broken_third = link(source, third);
        broken_third.target_status = Some(404);
        let mut broken_fourth = link(source, fourth);
        broken_fourth.target_status = Some(410);
        let facts = CrawlFacts::new(
            vec![page(source, 0)],
            vec![
                broken_third,
                broken_first,
                broken_second,
                broken_fourth,
                link(source, first),
            ],
        );

        let issues = derive(&facts);
        let issue = issues
            .iter()
            .find(|issue| issue.issue_type.id() == "internal_links_to_broken_pages")
            .expect("broken-target issue");

        assert_eq!(issue.severity, Severity::High);
        assert_eq!(
            issue.details,
            "Page links to 4 broken internal target(s): https://example.com/first, https://example.com/fourth, https://example.com/second, and 1 more."
        );
    }
}
