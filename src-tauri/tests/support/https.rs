//! Poll the actual verified HTTPS boundary while Caddy asynchronously issues
//! its local certificate. Caller supplies its CA/hostname-verifying client.
use reqwest::blocking::{Client, Response};
use std::time::{Duration, Instant};

pub fn ready_get(client: &Client, url: &str) -> reqwest::Result<Response> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match client.get(url).send() {
            Ok(response) => return Ok(response), // HTTP errors are caller assertions, never retries.
            Err(error) if error.is_connect() && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }
}
