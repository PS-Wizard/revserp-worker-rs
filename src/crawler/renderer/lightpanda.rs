use crate::crawler::client::MAX_BODY_SIZE;
use anyhow::{Context, Result};
use reqwest::{StatusCode, Url};
use serde::Deserialize;

use std::process::{ExitStatus, Output, Stdio};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tokio::time::timeout;

const MAX_RENDERED_OUTPUT_SIZE: usize = 16 * 1024 * 1024;

pub struct LightPandaSpawnConfig {
    binary: PathBuf,
    timeout: Duration,
    kill_timeout: Duration,
    max_output_bytes: usize,
}

#[derive(Deserialize)]
struct LightPandaJsonOutput {
    url: String,
    http_status: u16,
    dump: String,
    content: String,
}

pub struct LightPandaRenderedDocument {
    pub final_url: Url,
    pub status_code: StatusCode,
    pub html: Vec<u8>,
    pub process_status: ExitStatus,
    pub stderr: Vec<u8>,
}

fn parse_lightpanda_output(out: Output) -> Result<LightPandaRenderedDocument> {
    let Output {
        status,
        stdout,
        stderr,
    } = out;

    let output_size = stdout.len().saturating_add(stderr.len());

    anyhow::ensure!(
        output_size <= MAX_RENDERED_OUTPUT_SIZE,
        "Lightpanda output exceeded {MAX_RENDERED_OUTPUT_SIZE} bytes"
    );

    let parsed: LightPandaJsonOutput = serde_json::from_slice(&stdout)
        .context(format!("Failed to Parse Lightpanda Json, status: {status}"))?;

    anyhow::ensure!(
        parsed.content.len() <= MAX_BODY_SIZE,
        "Lightpanda output exceeded {MAX_RENDERED_OUTPUT_SIZE} bytes"
    );

    anyhow::ensure!(
        parsed.dump == "html",
        "Lightpanda returned an unexpected dumptype: {}",
        parsed.dump
    );

    let final_url = Url::parse(&parsed.url)?;
    let status_code = StatusCode::from_u16(parsed.http_status)?;
    let html = parsed.content.into_bytes();

    Ok(LightPandaRenderedDocument {
        final_url,
        status_code,
        html,
        process_status: status,
        stderr,
    })
}

impl LightPandaSpawnConfig {
    pub fn new() -> Self {
        LightPandaSpawnConfig {
            binary: Path::new("lightpanda").into(),
            timeout: Duration::from_secs(6),
            kill_timeout: Duration::from_secs(7),
            max_output_bytes: MAX_BODY_SIZE,
        }
    }

    pub async fn render(&self, url: &Url) -> Result<LightPandaRenderedDocument> {
        let timeout_ms = self.timeout.as_millis();
        let mut command = Command::new(&self.binary);

        command
            .arg("fetch")
            .arg(url.as_str())
            .arg("--json")
            .arg("--dump")
            .arg("html")
            .arg("--wait-until")
            .arg("done")
            .arg("--terminate-ms")
            .arg(timeout_ms.to_string())
            .arg("--http-max-response-size")
            .arg(self.max_output_bytes.to_string())
            .arg("--block-private-networks")
            .arg("--log-level")
            .arg("error")
            .env("LIGHTPANDA_DISABLE_TELEMETRY", "true")
            .env("LIGHTPANDA_DISABLE_CORE_DUMP", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = command.spawn().context("Failed To Start LightPanda")?;

        let output = timeout(self.kill_timeout, child.wait_with_output())
            .await
            .context("Lightpanda Exceeded Timeout")?
            .context("Failed To Wait For LightPanda")?;

        parse_lightpanda_output(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires the Lightpanda binary and internet access"]
    async fn renders_example_page() {
        let renderer = LightPandaSpawnConfig::new();
        let url = Url::parse("https://example.com").unwrap();

        let document = renderer.render(&url).await.unwrap();

        eprintln!("process status: {}", document.process_status);
        eprintln!("HTTP status: {}", document.status_code);
        eprintln!("final URL: {}", document.final_url);
        eprintln!("stderr: {}", String::from_utf8_lossy(&document.stderr));
        eprintln!("rendered bytes: {}", document.html.len());
        eprintln!(
            "HTML preview: {}",
            String::from_utf8_lossy(&document.html[..document.html.len().min(500)])
        );

        assert_eq!(document.status_code, StatusCode::OK);
        assert_eq!(document.final_url.as_str(), "https://example.com/");
        assert!(String::from_utf8_lossy(&document.html).contains("Example Domain"));
    }
}
