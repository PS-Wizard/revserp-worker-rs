use std::collections::HashSet;

use crate::crawler::facts::PageFact;

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const MISSING_H1: IssueType = IssueType::new(Pillar::Seo, "content_structure", "missing_h1");
const MULTIPLE_H1: IssueType = IssueType::new(Pillar::Seo, "content_structure", "multiple_h1");
const TITLE_H1_MISMATCH: IssueType =
    IssueType::new(Pillar::Seo, "content_structure", "title_h1_mismatch");
const MISSING_H2_ON_LONG_PAGE: IssueType =
    IssueType::new(Pillar::Seo, "content_structure", "missing_h2_on_long_page");
const SKIPPED_HEADING_LEVELS: IssueType =
    IssueType::new(Pillar::Seo, "content_structure", "skipped_heading_levels");

const TITLE_H1_MISMATCH_SIMILARITY_THRESHOLD: f64 = 0.32;

pub(super) fn derive(page: &PageFact) -> Vec<DerivedIssue> {
    let mut issues = Vec::new();

    if page.primary_h1.trim().is_empty() {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_H1,
            Severity::High,
            "Page is missing an H1",
            "Add one primary H1 heading to the page.".to_owned(),
        ));
    }
    if page.h1_count > 1 {
        issues.push(DerivedIssue::new(
            &page.url,
            MULTIPLE_H1,
            Severity::Medium,
            "Page has multiple H1 headings",
            "Keep one primary H1 heading per page.".to_owned(),
        ));
    }
    if title_h1_mismatch(&page.title, &page.primary_h1) {
        issues.push(DerivedIssue::new(
            &page.url,
            TITLE_H1_MISMATCH,
            Severity::Medium,
            "Page title and H1 do not align",
            format!(
                "Title {:?} does not closely match H1 {:?}.",
                page.title.trim(),
                page.primary_h1.trim()
            ),
        ));
    }
    if page.word_count >= 300 && page.h2_count == 0 {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_H2_ON_LONG_PAGE,
            Severity::Medium,
            "Long page is missing H2 headings",
            format!("Page has {} words but no H2 subheadings.", page.word_count),
        ));
    }
    if let Some(details) = skipped_heading_level_details(&page.heading_outline) {
        issues.push(DerivedIssue::new(
            &page.url,
            SKIPPED_HEADING_LEVELS,
            Severity::Medium,
            "Page skips heading levels",
            details,
        ));
    }

    issues
}

fn title_h1_mismatch(title: &str, h1: &str) -> bool {
    let normalized_title = normalize_whitespace(title);
    let normalized_h1 = normalize_whitespace(h1);
    if normalized_title.is_empty() || normalized_h1.is_empty() {
        return false;
    }
    if normalized_title == normalized_h1
        || normalized_title.contains(&normalized_h1)
        || normalized_h1.contains(&normalized_title)
    {
        return false;
    }

    character_trigram_similarity(&normalize_tokens(title), &normalize_tokens(h1))
        < TITLE_H1_MISMATCH_SIMILARITY_THRESHOLD
}

fn normalize_whitespace(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_tokens(value: &str) -> String {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "is", "it", "of",
        "on", "or", "our", "that", "the", "their", "this", "to", "we", "with", "you", "your",
    ];

    value
        .to_lowercase()
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(|character: char| {
                !character.is_alphabetic() && !character.is_numeric()
            });
            (!token.is_empty() && !STOP_WORDS.contains(&token)).then_some(token)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn character_trigram_similarity(left: &str, right: &str) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }

    let left_trigrams = character_trigrams(left);
    let right_trigrams = character_trigrams(right);
    let shared = left_trigrams.intersection(&right_trigrams).count();
    if shared == 0 {
        return 0.0;
    }
    (2 * shared) as f64 / (left_trigrams.len() + right_trigrams.len()) as f64
}

fn character_trigrams(value: &str) -> HashSet<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() < 3 {
        return [bytes.to_vec()].into_iter().collect();
    }
    bytes.windows(3).map(<[u8]>::to_vec).collect()
}

fn skipped_heading_level_details(outline: &[(u8, String)]) -> Option<String> {
    let details: Vec<_> = outline
        .windows(2)
        .filter_map(|headings| {
            let (previous_level, previous_text) = &headings[0];
            let (current_level, current_text) = &headings[1];
            if *previous_level == 0 || *current_level == 0 {
                return None;
            }
            (*current_level > previous_level.saturating_add(1)).then(|| {
                format!(
                    "H{} {:?} jumps to H{} {:?}.",
                    previous_level, previous_text, current_level, current_text
                )
            })
        })
        .collect();
    (!details.is_empty()).then(|| details.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> PageFact {
        PageFact {
            url: "https://example.com/page".to_owned(),
            content_type: Some("text/html".to_owned()),
            ..PageFact::default()
        }
    }

    fn issue_ids(page: &PageFact) -> Vec<&'static str> {
        derive(page)
            .iter()
            .map(|issue| issue.issue_type.id())
            .collect()
    }

    #[test]
    fn derives_missing_h1() {
        let issues = derive(&page());
        assert_eq!(issues[0].issue_type.id(), "missing_h1");
        assert_eq!(issues[0].severity, Severity::High);
        assert_eq!(issues[0].message, "Page is missing an H1");
        assert_eq!(issues[0].details, "Add one primary H1 heading to the page.");
    }

    #[test]
    fn derives_multiple_h1() {
        let mut page = page();
        page.primary_h1 = "Primary".to_owned();
        page.h1_count = 2;
        assert_eq!(issue_ids(&page), vec!["multiple_h1"]);
    }

    #[test]
    fn derives_title_h1_mismatch() {
        let mut page = page();
        page.title = "Buy running shoes online".to_owned();
        page.primary_h1 = "About our company".to_owned();
        page.h1_count = 1;
        let issues = derive(&page);
        assert_eq!(issue_ids(&page), vec!["title_h1_mismatch"]);
        assert_eq!(
            issues[0].details,
            "Title \"Buy running shoes online\" does not closely match H1 \"About our company\"."
        );
    }

    #[test]
    fn derives_missing_h2_on_long_page() {
        let mut page = page();
        page.primary_h1 = "A heading".to_owned();
        page.h1_count = 1;
        page.word_count = 300;
        let issues = derive(&page);
        assert_eq!(issue_ids(&page), vec!["missing_h2_on_long_page"]);
        assert_eq!(
            issues[0].details,
            "Page has 300 words but no H2 subheadings."
        );
    }

    #[test]
    fn derives_skipped_heading_levels() {
        let mut page = page();
        page.primary_h1 = "Overview".to_owned();
        page.h1_count = 1;
        page.heading_outline = vec![(2, "Overview".to_owned()), (4, "Details".to_owned())];
        let issues = derive(&page);
        assert_eq!(issue_ids(&page), vec!["skipped_heading_levels"]);
        assert_eq!(
            issues[0].details,
            "H2 \"Overview\" jumps to H4 \"Details\"."
        );
    }
    #[test]
    fn skips_heading_issues_for_broken_and_non_html_pages() {
        let facts = crate::crawler::facts::CrawlFacts::new(
            vec![
                PageFact {
                    url: "https://example.com/broken".to_owned(),
                    status_code: 404,
                    content_type: Some("text/html".to_owned()),
                    ..PageFact::default()
                },
                PageFact {
                    url: "https://example.com/document".to_owned(),
                    content_type: Some("application/pdf".to_owned()),
                    ..PageFact::default()
                },
            ],
            Vec::new(),
        );

        let issues = crate::issues::derive_issues(&facts);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type.id(), "client_error_status");
    }
}
