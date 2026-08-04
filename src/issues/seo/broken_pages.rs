use crate::crawler::facts::{CrawlFacts, PageFact};

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const FETCH_FAILED: IssueType = IssueType::new(Pillar::Seo, "technical_seo", "fetch_failed");
const SOFT_404: IssueType = IssueType::new(Pillar::Seo, "technical_seo", "soft_404");
const SERVER_ERROR_STATUS: IssueType =
    IssueType::new(Pillar::Seo, "technical_seo", "server_error_status");
const CLIENT_ERROR_STATUS: IssueType =
    IssueType::new(Pillar::Seo, "technical_seo", "client_error_status");

pub(super) fn derive(facts: &CrawlFacts) -> Vec<DerivedIssue> {
    facts
        .pages
        .iter()
        .filter_map(derive_broken_page_issue)
        .collect()
}

fn derive_broken_page_issue(page: &PageFact) -> Option<DerivedIssue> {
    if !page.is_scoreable_content_type() {
        return None;
    }

    if let Some(error) = &page.fetch_error {
        return Some(DerivedIssue::new(
            &page.url,
            FETCH_FAILED,
            Severity::High,
            "Page could not be fetched",
            format!("The crawler could not retrieve this page: {error}."),
        ));
    }

    if page.soft_404 {
        return Some(DerivedIssue::new(
            &page.url,
            SOFT_404,
            Severity::High,
            "Page returns a not-found message with a success status",
            format!(
                "Page answered HTTP {} but serves the site's \"not found\" content. Search engines treat this as a soft 404 and may drop the URL without reporting an error. Return a real 404 or 410 status for URLs that do not exist.",
                page.status_code
            ),
        ));
    }

    if page.status_code >= 500 {
        return Some(DerivedIssue::new(
            &page.url,
            SERVER_ERROR_STATUS,
            Severity::High,
            "Page returned a server error",
            format!("Page returned HTTP {}.", page.status_code),
        ));
    }

    if page.status_code >= 400 {
        return Some(DerivedIssue::new(
            &page.url,
            CLIENT_ERROR_STATUS,
            Severity::High,
            "Page returned a client error",
            format!("Page returned HTTP {}.", page.status_code),
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, status_code: u16, fetch_error: Option<&str>, soft_404: bool) -> PageFact {
        PageFact {
            url: url.to_owned(),
            status_code,
            fetch_error: fetch_error.map(str::to_owned),
            soft_404,
            content_type: Some("text/html".to_owned()),
            title: "Page".to_owned(),
            primary_h1: "Page".to_owned(),
            h1_count: 1,
            ..PageFact::default()
        }
    }

    #[test]
    fn derives_one_issue_for_each_broken_page_and_ignores_healthy_pages() {
        let facts = CrawlFacts {
            pages: vec![
                page("fetch", 500, Some("request timed out"), true),
                page("soft", 500, None, true),
                page("server", 503, None, false),
                page("client", 404, None, false),
                page("healthy", 200, None, false),
            ],
            links: Vec::new(),
        };

        let issues = derive(&facts);

        assert_eq!(
            issues
                .iter()
                .map(|issue| (
                    issue.url.as_str(),
                    issue.issue_type.pillar(),
                    issue.issue_type.bucket(),
                    issue.issue_type.id(),
                    issue.severity,
                    issue.message.as_str(),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    "fetch",
                    Pillar::Seo,
                    "technical_seo",
                    "fetch_failed",
                    Severity::High,
                    "Page could not be fetched",
                ),
                (
                    "soft",
                    Pillar::Seo,
                    "technical_seo",
                    "soft_404",
                    Severity::High,
                    "Page returns a not-found message with a success status",
                ),
                (
                    "server",
                    Pillar::Seo,
                    "technical_seo",
                    "server_error_status",
                    Severity::High,
                    "Page returned a server error",
                ),
                (
                    "client",
                    Pillar::Seo,
                    "technical_seo",
                    "client_error_status",
                    Severity::High,
                    "Page returned a client error",
                ),
            ]
        );
    }
}
