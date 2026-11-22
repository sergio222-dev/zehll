//! Clipboard access for the launcher.

use tokio::process::Command;

/// Puts `text` on the clipboard with `wl-copy`.
pub async fn copy(text: String) {
    // Awaited so the exited wl-copy is reaped; it forks to keep serving the selection.
    match Command::new("wl-copy").arg("--").arg(&text).status().await {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("zehll: wl-copy failed: {status}"),
        Err(err) => eprintln!("zehll: cannot run wl-copy: {err}"),
    }
}
