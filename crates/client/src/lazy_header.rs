//! Lazy extraction header logging.
//!
//! When a third-party crate (e.g. `lopdf`, `sevenz_rust2`) emits a WARN-or-above
//! log during file extraction, it provides no path context. This module emits a
//! single `INFO Processing <path>` line before the first such message, so the
//! user can identify which file triggered the warning without the path being
//! repeated on every line.
//!
//! Usage:
//!   1. Register `FileHeaderLayer` in the tracing subscriber stack.
//!   2. Call `set_pending(path)` immediately before extracting a file.
//!   3. Call `clear_pending()` immediately after (success or error).
//!
//! Our own `warn!` calls already include the path in the message body, so the
//! header is suppressed for events whose target starts with `find_`.

use std::cell::RefCell;

use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, Layer};

// ── Thread-local state ────────────────────────────────────────────────────────

struct Pending {
    path: String,
    emitted: bool,
}

thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
    // Re-entrancy guard: prevents the `tracing::info!` inside `on_event` from
    // triggering another `on_event` → infinite recursion.
    static IN_HEADER: RefCell<bool> = const { RefCell::new(false) };
}

/// Call this immediately before extracting a file on the current thread.
pub fn set_pending(path: &str) {
    PENDING.with(|p| {
        *p.borrow_mut() = Some(Pending { path: path.to_owned(), emitted: false });
    });
}

/// Call this immediately after extraction completes (success or error).
pub fn clear_pending() {
    PENDING.with(|p| *p.borrow_mut() = None);
}

// ── Layer ─────────────────────────────────────────────────────────────────────

pub struct FileHeaderLayer;

impl<S: Subscriber> Layer<S> for FileHeaderLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Only care about WARN and above.
        if *event.metadata().level() > tracing::Level::WARN {
            return;
        }
        // Our own code already includes the path in the message — skip.
        if event.metadata().target().starts_with("find_") {
            return;
        }
        // Avoid re-entrancy from the tracing::info! call below.
        let already_in_header = IN_HEADER.with(|h| *h.borrow());
        if already_in_header {
            return;
        }

        PENDING.with(|p| {
            let mut pending = p.borrow_mut();
            if let Some(ref mut hdr) = *pending {
                if !hdr.emitted {
                    hdr.emitted = true;
                    let path = hdr.path.clone();
                    // Drop the borrow before calling tracing::info! to avoid
                    // a BorrowMutError if the subscriber re-enters on_event.
                    drop(pending);
                    IN_HEADER.with(|h| *h.borrow_mut() = true);
                    tracing::info!(target: "find_scan::scan", "Processing {path}");
                    IN_HEADER.with(|h| *h.borrow_mut() = false);
                }
            }
        });
    }
}
