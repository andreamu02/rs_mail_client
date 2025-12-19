use anyhow::Result;
use notify_rust::Notification;

use crate::domain::email::EmailSummary;

pub struct Notifier;

impl Notifier {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn notify_email(&self, email: &EmailSummary) -> Result<()> {
        // Best-effort: actions work on some systems (Linux/DBus).
        // If actions don't work, you still get the notification.
        let exe = std::env::current_exe().ok();

        let mut n = Notification::new();
        n.summary(&email.subject)
            .body(&email.snippet)
            .action("open", "Open")
            .action("dismiss", "Dismiss");

        let handle = match n.show() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Notification error: {e}");
                return Ok(());
            }
        };

        if let Some(exe) = exe {
            let uid = email.id;
            std::thread::spawn(move || {
                if let Ok(action) = handle.wait_for_action(|a| a.to_string()) {
                    if action == "open" {
                        let _ = std::process::Command::new(exe)
                            .arg("tui")
                            .arg("--open")
                            .arg(uid.to_string())
                            .spawn();
                    }
                }
            });
        }

        Ok(())
    }
}
