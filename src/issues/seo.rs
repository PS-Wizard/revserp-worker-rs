use crate::crawler::facts::CrawlFacts;

use super::DerivedIssue;

mod broken_pages;
mod content_quality;
mod headings;
mod indexability;
mod internal_linking;
mod media_optimization;
mod serp_metadata;
mod technical_seo;

pub(super) fn derive(facts: &CrawlFacts) -> Vec<DerivedIssue> {
    let mut issues = broken_pages::derive(facts);
    for page in facts
        .pages
        .iter()
        .filter(|page| page.is_healthy() && page.is_scoreable_content_type())
    {
        issues.extend(indexability::derive(page, facts));
        issues.extend(serp_metadata::derive(page));
        issues.extend(technical_seo::derive(page));
        issues.extend(media_optimization::derive(page));
        issues.extend(headings::derive(page));
        issues.extend(content_quality::derive(page));
    }
    issues.extend(internal_linking::derive(facts));
    issues
}
