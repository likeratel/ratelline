//! Delivers macOS notifications one at a time from a single thread.
//!
//! `tauri-plugin-notification` spawns every `show()` onto its own runtime task,
//! so a burst of notifications reaches `NSUserNotificationCenter` (through
//! mac-notification-sys) from several threads at once. That API is not
//! documented as thread-safe, so one dedicated sender keeps the calls serial.

use std::sync::{mpsc, OnceLock};

struct Request {
    title: String,
    body: String,
}

/// Queues a notification. Returns once queued; delivery happens in order on
/// the sender thread.
pub(super) fn deliver(bundle_identifier: &str, title: &str, body: &str) -> Result<(), String> {
    static SENDER: OnceLock<Option<mpsc::Sender<Request>>> = OnceLock::new();
    let sender = SENDER
        .get_or_init(|| start(bundle_identifier.to_owned()))
        .as_ref()
        .ok_or_else(|| "The notification sender could not be started.".to_owned())?;
    sender
        .send(Request {
            title: title.to_owned(),
            body: body.to_owned(),
        })
        .map_err(|_| "The notification sender has stopped.".to_owned())
}

fn start(bundle_identifier: String) -> Option<mpsc::Sender<Request>> {
    let (sender, receiver) = mpsc::channel::<Request>();
    let spawned = std::thread::Builder::new()
        .name("ratelline-notifications".into())
        .spawn(move || {
            // Same identity choice as the plugin: dev builds have no registered bundle.
            let application = if tauri::is_dev() {
                "com.apple.Terminal"
            } else {
                bundle_identifier.as_str()
            };
            if let Err(error) = notify_rust::set_application(application) {
                eprintln!("Could not set the notification application: {error}");
            }
            for request in receiver {
                // On macOS `show()` only builds a handle; dropping it at the end of
                // this statement sends the notification and waits briefly for
                // delivery, so the next request starts after this one. notify-rust
                // discards send errors there, as the plugin path did.
                if let Err(error) = notify_rust::Notification::new()
                    .summary(&request.title)
                    .body(&request.body)
                    .show()
                {
                    eprintln!("A system notification could not be delivered: {error}");
                }
            }
        });
    match spawned {
        Ok(_) => Some(sender),
        Err(error) => {
            eprintln!("Could not start the notification sender: {error}");
            None
        }
    }
}
