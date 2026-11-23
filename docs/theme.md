# Theme

[← Back to the README](../README.md)

The bar uses a tactical HUD style: a fixed navy base with slanted panels and glowing highlights. Its accent colors come from a Material 3 palette in matugen's `colors.json` format, read from `~/.config/zehll/theme.json` (or `$XDG_CONFIG_HOME/zehll/theme.json`). Missing keys fall back to orange and cyan.

| Role | Material 3 token |
|---|---|
| Accent (window tag, clock underline, highlights) | `primary` |
| Text on the accent | `on_primary` |
| Hover glow | `tertiary` |

The Barlow and Barlow Condensed fonts are embedded in the binary (SIL Open Font License, see [`assets/fonts/OFL.txt`](../assets/fonts/OFL.txt)).

Values that update on their own (the system indicator, the system tooltip and popover) and code or data in the launcher (calculator, clipboard, keybinds) use [MonoLisa Code](https://www.monolisa.dev), so digits keep a fixed width and stop shifting. MonoLisa is a commercial font: Zehll asks for the family `MonoLisaCode` from the system fonts and never embeds it. Install it under `~/.local/share/fonts` (the Upright and Italic variable TTFs) and run `fc-cache -f`. Without it, that text falls back to another face.

To regenerate it with the wallpaper, add a template to `~/.config/matugen/config.toml` that reuses a `colors.json` template:

```toml
[templates.zehll]
input_path = '~/.config/matugen/templates/colors.json'
output_path = '~/.config/zehll/theme.json'
```

## Requirements

- [matugen](https://github.com/InioX/matugen), for wallpaper-based colors
- [MonoLisa Code](https://www.monolisa.dev), for numbers and code (optional, licensed separately)
