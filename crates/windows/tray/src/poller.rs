//! Background thread that polls the Windows SCM for service state and the
//! find-anything server for file counts, then sends status updates to the
//! main thread via the winit EventLoopProxy.

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::AppEvent;
use crate::service_ctl;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Spawn the background poller thread.  Events are sent on `tx`.
pub fn spawn(tx: Sender<AppEvent>, server_url: String, token: String) {
    thread::Builder::new()
        .name("find-tray-poller".into())
        .spawn(move || run(tx, server_url, token))
        .expect("spawning poller thread");
}

fn run(tx: Sender<AppEvent>, server_url: String, token: String) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    loop {
        let service_running = service_ctl::is_service_running();
        let (file_count, source_count) = query_server(&client, &server_url, &token);

        let event = AppEvent::StatusUpdate {
            service_running,
            file_count,
            source_count,
        };

        if tx.send(event).is_err() {
            // Main thread has exited; stop polling.
            break;
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn query_server(
    client: &reqwest::blocking::Client,
    server_url: &str,
    token: &str,
) -> (Option<u64>, Option<usize>) {
    let url = format!("{server_url}/api/v1/sources");
    let resp = match client
        .get(&url)
        .bearer_auth(token)
        .send()
    {
        Ok(r) => r,
        Err(_) => return (None, None),
    };

    if !resp.status().is_success() {
        return (None, None);
    }

    // Parse the sources response: array of objects with `file_count` field.
    let json: serde_json::Value = match resp.json() {
        Ok(v) => v,
        Err(_) => return (None, None),
    };

    if let Some(sources) = json.as_array() {
        let total_files: u64 = sources
            .iter()
            .filter_map(|s| s.get("file_count").and_then(|v| v.as_u64()))
            .sum();
        (Some(total_files), Some(sources.len()))
    } else {
        (None, None)
    }
}
