# Notifications

[← Back to the README](../README.md)

Zehll is a notification daemon (`org.freedesktop.Notifications`); new notifications pop up at the top right (up to three, 5 s by default, critical ones until dismissed) and are listed in the side panel, with do not disturb and clear all. Notifications show their image or app icon and their action buttons; clicking one runs its default action (or hides the popup when it has none), and ✕ dismisses it. If another daemon such as swaync owns the name, Zehll waits in the D-Bus queue and takes over when that daemon exits, so disable the other daemon to use Zehll's.
