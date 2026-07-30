mod client;
mod extract;
mod fetch;
mod runner;

pub use client::FetchClient;
pub use fetch::fetch_url;
pub use runner::Crawler;
