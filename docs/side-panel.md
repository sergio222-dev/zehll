# Side panel

[← Back to the README](../README.md)

Hover the right edge of the bar to open the side panel. Click outside it or press Escape to close it. Zehll registers the Hyprland global shortcut `zehll:panelToggle`, so a keybind can toggle it:

```
bind = SUPER, N, global, zehll:panelToggle
```

To slide the panel in from the right, add Hyprland layer rules for its namespaces:

```
layerrule = animation slide right, ^zehll-panel$
layerrule = noanim, ^zehll-panel-catcher$
```

The power button runs [wlogout](https://github.com/ArtsyMacaw/wlogout).

Quick settings toggle Wi-Fi, Bluetooth, audio output and microphone mute, night light and keep awake:

- Clicking a tile's icon toggles it; clicking the rest of the tile opens its sub-panel: Wi-Fi networks (connect, with a password prompt for new secured ones), Bluetooth devices (connect, pair, disconnect, forget, always connect) or the audio mixer (per-app volume sliders and output device). Details opens KDE's network or Bluetooth settings, or pavucontrol.
- The night light tile opens eye protection: night light intensity and schedule, an anti-flashbang screen shader that dims bright screens, monitor brightness over DDC/CI ([ddcutil](https://www.ddcutil.com/)) and gamma.
- Night light switches [hyprsunset](https://github.com/hyprwm/hyprsunset) between 5000 K and 6000 K. It turns on automatically from 19:00 to 06:30; a manual toggle holds until the next change of the schedule.
- Keep awake holds a logind idle inhibitor, which hypridle honors. It is remembered across restarts in `~/.local/state/zehll/state.json`.

## Requirements

- [wlogout](https://github.com/ArtsyMacaw/wlogout) for the power button
- [hyprsunset](https://github.com/hyprwm/hyprsunset) for night light
- [ddcutil](https://www.ddcutil.com/) for monitor brightness
