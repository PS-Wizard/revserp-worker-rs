use reqwest::{Client, ClientBuilder, Url, redirect::Policy};

use super::{
    scope::hosts_equivalent,
    ssrf::{SafeResolver, validate_url},
};

// 10 MiB
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

pub struct FetchClient {
    pub(super) client: Client,
    pub(super) max_body_size: usize,
    allow_loopback: bool,
}

impl FetchClient {
    pub fn new() -> Self {
        Self::build(false)
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests() -> Self {
        Self::build(true)
    }

    fn build(allow_loopback: bool) -> Self {
        let redirect_limit = Policy::limited(10);
        let client = ClientBuilder::new()
            .redirect(Policy::custom(move |attempt| {
                if let Err(error) = validate_url(attempt.url(), allow_loopback) {
                    return attempt.error(error);
                }
                if !attempt
                    .previous()
                    .first()
                    .is_some_and(|initial_url| hosts_equivalent(initial_url, attempt.url()))
                {
                    return attempt.error("redirect crosses crawler host");
                }
                redirect_limit.redirect(attempt)
            }))
            .dns_resolver(SafeResolver::new(allow_loopback))
            .gzip(true)
            .build()
            .expect("Failed to create a new FetchClient");

        Self {
            client,
            max_body_size: MAX_BODY_SIZE,
            allow_loopback,
        }
    }

    pub(crate) fn validate_url(&self, url: &Url) -> anyhow::Result<()> {
        validate_url(url, self.allow_loopback)
    }
}
