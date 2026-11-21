use std::sync::Arc;

use zbus::Connection;
use zbus::zvariant::OwnedFd;

use crate::dbus;

/// Takes a logind "idle" inhibitor lock, which hypridle honors. The lock lasts until the
/// returned descriptor is dropped.
pub async fn inhibit() -> Option<Arc<OwnedFd>> {
    let result = async {
        let connection = Connection::system().await?;
        let manager =
            dbus::proxy(&connection, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").await?;
        let lock: OwnedFd = manager.call("Inhibit", &("idle", "Zehll", "Keep awake", "block")).await?;
        zbus::Result::Ok(lock)
    };
    match result.await {
        Ok(lock) => Some(Arc::new(lock)),
        Err(err) => {
            eprintln!("zehll: cannot inhibit idle: {err}");
            None
        }
    }
}
