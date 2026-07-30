use reqwest::{Client, ClientBuilder, redirect::Policy};

// 10 MiB
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

pub struct FetchClient {
    pub client: Client,
    pub max_body_size: usize,
    // retry config
    // max redirects
    // timeout duration
    // ...
}

impl FetchClient {
    pub fn new() -> Self {
        let client = ClientBuilder::new()
            .redirect(Policy::none())
            .gzip(true)
            .build()
            .expect("Failed to create a new FetchClient");

        Self {
            client,
            max_body_size: MAX_BODY_SIZE,
        }
    }
}
