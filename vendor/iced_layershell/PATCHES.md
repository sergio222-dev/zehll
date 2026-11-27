# Zehll patches to iced_layershell 0.19.1

This is iced_layershell 0.19.1 from crates.io (MIT, see `LICENSE`), wired in
through `[patch.crates-io]` in Zehll's `Cargo.toml`. Drop the directory and the
patch section once an upstream release carries the fix below (it is on
upstream's master, merged after 0.19.1 shipped).

## Recover outdated and lost surfaces

`src/multi_window.rs`, the `compositor.present` error branch.

Backport of waycrate/exwlshelleventloop#401 (commit `072bfcd`), the fix for
issue #360. Upstream 0.19.1 only logs `SurfaceError::Outdated` and
`SurfaceError::Lost` through `tracing`. It never reconfigures the swapchain, so
every later present of that surface fails the same way and the layer keeps
showing its last frame. Zehll's DP-1 bar froze this way on NVIDIA (Vulkan): the
clock stuck at one minute, the other monitor's bar kept updating, and the
process sat idle. `Outdated` now reconfigures the surface and `Lost` recreates
it, and both request a redraw.

## Present errors on stderr

Zehll installs no `tracing` subscriber, so the same branch also prints every
present error with `eprintln!`, which lands in `~/.local/state/zehll/zehll.log`.
