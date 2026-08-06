use reqwest::Url;

use crate::crawler::facts::{CrawlFacts, PageFact};

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const MISSING_CANONICAL: IssueType =
    IssueType::new(Pillar::Seo, "indexability", "missing_canonical");
const MALFORMED_CANONICAL: IssueType =
    IssueType::new(Pillar::Seo, "indexability", "malformed_canonical");
const CANONICAL_DIFFERS: IssueType =
    IssueType::new(Pillar::Seo, "indexability", "canonical_differs");
const CANONICAL_POINTS_TO_NON_INDEXABLE_PAGE: IssueType = IssueType::new(
    Pillar::Seo,
    "indexability",
    "canonical_points_to_non_indexable_page",
);
const NOINDEX_PAGE: IssueType = IssueType::new(Pillar::Seo, "indexability", "noindex_page");
const NOFOLLOW_PAGE: IssueType = IssueType::new(Pillar::Seo, "indexability", "nofollow_page");

pub(super) fn derive(page: &PageFact, facts: &CrawlFacts) -> Vec<DerivedIssue> {
    let mut issues = Vec::new();
    let canonical = page.canonical_url.trim();

    if canonical.is_empty() {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_CANONICAL,
            Severity::Medium,
            "Page is missing a canonical URL",
            "Add a canonical link element for the preferred page URL.".to_owned(),
        ));
    } else if let Ok(url) = Url::parse(canonical)
        && matches!(url.scheme(), "http" | "https")
    {
        if page.url.trim() != canonical {
            issues.push(DerivedIssue::new(
                &page.url,
                CANONICAL_DIFFERS,
                Severity::Low,
                "Canonical URL differs from page URL",
                format!("Canonical points to {canonical}."),
            ));
        }
        if facts.page_by_url(canonical).is_some_and(|target| {
            target.status_code >= 400 || target.robots.to_ascii_lowercase().contains("noindex")
        }) {
            issues.push(DerivedIssue::new(
                &page.url,
                CANONICAL_POINTS_TO_NON_INDEXABLE_PAGE,
                Severity::High,
                "Canonical points to a non-indexable page",
                format!("Canonical target {canonical} is non-indexable."),
            ));
        }
    } else {
        issues.push(DerivedIssue::new(
            &page.url,
            MALFORMED_CANONICAL,
            Severity::High,
            "Canonical URL is malformed",
            format!("Canonical value {canonical:?} is not a valid absolute HTTP URL."),
        ));
    }

    let robots = page.robots.to_ascii_lowercase();
    if robots.contains("noindex") {
        issues.push(DerivedIssue::new(
            &page.url,
            NOINDEX_PAGE,
            Severity::High,
            "Page is marked noindex",
            "Remove the noindex directive if the page should appear in search results.".to_owned(),
        ));
    }
    if robots.contains("nofollow") {
        issues.push(DerivedIssue::new(
            &page.url,
            NOFOLLOW_PAGE,
            Severity::Medium,
            "Page is marked nofollow",
            "Remove the nofollow directive if search engines should follow links on this page."
                .to_owned(),
        ));
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(canonical: &str) -> PageFact {
        PageFact {
            url: "https://example.com/page".to_owned(),
            status_code: 200,
            content_type: Some("text/html".to_owned()),
            canonical_url: canonical.to_owned(),
            ..PageFact::default()
        }
    }

    fn facts(pages: Vec<PageFact>) -> CrawlFacts {
        CrawlFacts::new(pages, Vec::new())
    }

    fn ids(issues: &[DerivedIssue]) -> Vec<&'static str> {
        issues.iter().map(|issue| issue.issue_type.id()).collect()
    }

    #[test]
    fn derives_missing_canonical_with_exact_copy() {
        let issues = derive(&page("  "), &facts(Vec::new()));

        assert_eq!(
            (
                issues[0].issue_type.pillar(),
                issues[0].issue_type.bucket(),
                issues[0].issue_type.id(),
                issues[0].severity,
                issues[0].message.as_str(),
                issues[0].details.as_str(),
            ),
            (
                Pillar::Seo,
                "indexability",
                "missing_canonical",
                Severity::Medium,
                "Page is missing a canonical URL",
                "Add a canonical link element for the preferred page URL.",
            )
        );
    }

    #[test]
    fn rejects_malformed_and_non_http_canonicals() {
        for canonical in ["not a URL", "mailto:test@example.com"] {
            let issues = derive(&page(canonical), &facts(Vec::new()));
            assert_eq!(ids(&issues), vec!["malformed_canonical"]);
            assert_eq!(issues[0].severity, Severity::High);
        }
    }

    #[test]
    fn derives_low_severity_difference_but_not_for_self_reference() {
        let self_reference = derive(&page("https://example.com/page"), &facts(Vec::new()));
        let differs = derive(&page("https://example.com/preferred"), &facts(Vec::new()));

        assert!(self_reference.is_empty());
        assert_eq!(ids(&differs), vec!["canonical_differs"]);
        assert_eq!(differs[0].severity, Severity::Low);
    }

    #[test]
    fn derives_target_issue_for_broken_or_noindex_canonical_targets() {
        for target in [
            PageFact {
                url: "https://example.com/target".to_owned(),
                status_code: 404,
                ..PageFact::default()
            },
            PageFact {
                url: "https://example.com/target".to_owned(),
                status_code: 200,
                robots: "NOINDEX, follow".to_owned(),
                ..PageFact::default()
            },
        ] {
            let issues = derive(&page("https://example.com/target"), &facts(vec![target]));
            assert_eq!(
                ids(&issues),
                vec![
                    "canonical_differs",
                    "canonical_points_to_non_indexable_page"
                ]
            );
            assert_eq!(issues[1].severity, Severity::High);
        }
    }

    #[test]
    fn unknown_canonical_target_is_not_called_broken() {
        let issues = derive(&page("https://other.example/page"), &facts(Vec::new()));

        assert_eq!(ids(&issues), vec!["canonical_differs"]);
    }

    #[test]
    fn derives_robots_issues_case_insensitively_after_canonical_issue() {
        let mut page = page("");
        page.robots = "NOINDEX, NOFOLLOW".to_owned();

        let issues = derive(&page, &facts(Vec::new()));

        assert_eq!(
            ids(&issues),
            vec!["missing_canonical", "noindex_page", "nofollow_page"]
        );
    }

    #[test]
    fn coordinator_derives_indexability_only_for_healthy_html_sources() {
        let healthy = page("");
        let mut broken = page("");
        broken.url = "https://example.com/broken".to_owned();
        broken.status_code = 404;
        let mut non_html = page("");
        non_html.url = "https://example.com/document".to_owned();
        non_html.content_type = Some("application/pdf".to_owned());
        let facts = facts(vec![healthy, broken, non_html]);

        let issues = crate::issues::derive_issues(&facts);

        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.issue_type.bucket() == "indexability")
                .map(|issue| (issue.url.as_str(), issue.issue_type.id()))
                .collect::<Vec<_>>(),
            [("https://example.com/page", "missing_canonical")],
        );
    }
}
