use reqwest::{Client, ClientBuilder, Url, redirect::Policy};

use super::ssrf::{SafeResolver, validate_url};

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
                match validate_url(attempt.url(), allow_loopback) {
                    Ok(()) => redirect_limit.redirect(attempt),
                    Err(error) => attempt.error(error),
                }
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
