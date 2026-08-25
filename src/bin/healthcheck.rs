//! tiny healthcheck client used by the docker HEALTHCHECK.
//!
//! the runtime image ships no curl/wget; instead of adding packages we
//! reuse the already-compiled reqwest/rustls stack. exit 0 only when
//! /health/live answers 2xx.

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let port = std::env::var("DEKO_PORT").unwrap_or_else(|_| "8000".to_string());
    let url = format!("http://127.0.0.1:{}/health/live", port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .expect("healthcheck client");

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => std::process::exit(0),
        Ok(resp) => {
            eprintln!("healthcheck: {} answered {}", url, resp.status());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("healthcheck: {} unreachable: {}", url, e);
            std::process::exit(1);
        }
    }
}
