[← Back to the README](../README.md)

# Volume indicator

When the volume or mute of the default output changes, from the keyboard's volume keys, a headset button or any app, Zehll shows an indicator at the top center of the focused monitor, under the bar.

- It shows the device name, the volume and a 20-cell gauge (5 % per cell).
- Above 100 % (the volume keys go up to 150 %) the chip reads BOOST and the gauge and value turn yellow.
- Muted: a red MUTED chip, the value struck through and the gauge dimmed, keeping its level.
- It hides 1.5 s after the last change, so holding a key keeps it on screen. Touching it with the pointer hides it at once.
- It doesn't show at startup, when the default device switches, or while the side panel is open (its audio tile already shows the volume).

The surface's namespace is `zehll-osd`. Unlike the popovers, it keeps Hyprland's layer animation, so it comes and goes with your layer animation settings. Don't give it a `no_anim` rule like the launcher's.

## Requirements

- `pactl` (pipewire-pulse), to hear about volume changes
- `wpctl` (WirePlumber), to read the volume
