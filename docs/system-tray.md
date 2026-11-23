# System tray

[← Back to the README](../README.md)

Zehll registers as a StatusNotifierHost. It also queues for the `org.kde.StatusNotifierWatcher` name: if another shell already provides the watcher, Zehll uses it, and D-Bus hands the name over to Zehll when that shell exits.
