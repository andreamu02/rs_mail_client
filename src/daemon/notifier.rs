use anyhow::Result;
use notify_rust::{Hint, Notification};
use std::fs;

use crate::domain::email::EmailSummary;

pub struct Notifier {
    icon_path: String,
}

impl Notifier {
    pub fn new() -> Result<Self> {
        Ok(Self {
            icon_path: ensure_icon_path()?,
        })
    }

    pub fn notify_email(&self, email: &EmailSummary) -> Result<()> {
        let exe = std::env::current_exe().ok();
        let uid = email.id;

        let mut n = Notification::new();
        n.summary(&format!("{} — {}", email.from_name, email.subject))
            .body(&email.snippet)
            .icon(&self.icon_path) // absolute path works :contentReference[oaicite:1]{index=1}
            .hint(Hint::Category("email".to_string()))
            // “default” is the action some servers send when you click the notification body
            .action("default", "Open")
            // plus a visible button on servers that show actions
            .action("open", "Open");

        let handle = match n.show() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Notification error: {e}");
                return Ok(());
            }
        };

        if let Some(exe) = exe {
            std::thread::spawn(move || {
                // wait_for_action calls the closure with the action name :contentReference[oaicite:2]{index=2}
                handle.wait_for_action(|action| {
                    if action == "default" || action == "open" {
                        let _ = spawn_tui_in_terminal(&exe, uid);
                    }
                });
            });
        }

        Ok(())
    }
}

fn ensure_icon_path() -> Result<String> {
    let base = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("no config dir"))?
        .join("rs_mail_client");
    fs::create_dir_all(&base)?;

    let icon_file = base.join("icon.png");
    if !icon_file.exists() {
        // Embed the icon in the binary and write it out once
        let bytes: &[u8] = include_bytes!("../../assets/icon.png");
        fs::write(&icon_file, bytes)?;
    }

    Ok(icon_file.to_string_lossy().to_string())
}

fn spawn_tui_in_terminal(exe: &std::path::Path, uid: u32) -> Result<()> {
    // Pick terminal from env, fallback to common ones
    // Examples:
    //   RS_MAIL_CLIENT_TERMINAL=kitty
    //   RS_MAIL_CLIENT_TERMINAL=/usr/bin/foot
    let term = std::env::var("RS_MAIL_CLIENT_TERMINAL").ok();

    let candidates: Vec<String> = if let Some(t) = term {
        vec![t]
    } else {
        vec![
            "kitty".into(),
            "alacritty".into(),
            "foot".into(),
            "wezterm".into(),
            "gnome-terminal".into(),
            "konsole".into(),
            "xterm".into(),
        ]
    };

    let client = exe.to_string_lossy().to_string();
    let args = vec![client, "tui".into(), "--open".into(), uid.to_string()];

    for t in candidates {
        let mut cmd = std::process::Command::new(&t);

        if t.contains("gnome-terminal") {
            // gnome-terminal -- <cmd> <args...>
            cmd.arg("--");
            cmd.args(&args);
        } else if t.contains("wezterm") {
            // wezterm start -- <cmd> <args...>
            cmd.args(["start", "--"]);
            cmd.args(&args);
        } else if t.contains("konsole") {
            // konsole -e <cmd> <args...>
            cmd.arg("-e");
            cmd.args(&args);
        } else {
            // kitty/alacritty/foot/xterm: -e <cmd> <args...>
            cmd.arg("-e");
            cmd.args(&args);
        }

        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(_) => continue,
        }
    }

    Err(anyhow::anyhow!(
        "Could not launch a terminal. Set RS_MAIL_CLIENT_TERMINAL to your terminal emulator (e.g. kitty, foot, alacritty)."
    ))
}
