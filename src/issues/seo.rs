use crate::crawler::facts::CrawlFacts;

use super::DerivedIssue;

mod broken_pages;

pub(super) fn derive(facts: &CrawlFacts) -> Vec<DerivedIssue> {
    broken_pages::derive(facts)
}
