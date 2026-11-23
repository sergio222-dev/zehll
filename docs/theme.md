# Theme

[← Back to the README](../README.md)

The bar uses a tactical HUD style: a fixed navy base with slanted panels and glowing highlights. Its accent colors come from a Material 3 palette in matugen's `colors.json` format, read from `~/.config/zehll/theme.json` (or `$XDG_CONFIG_HOME/zehll/theme.json`). Missing keys fall back to orange and cyan.

| Role | Material 3 token |
|---|---|
| Accent (window tag, clock underline, highlights) | `primary` |
| Text on the accent | `on_primary` |
| Hover glow | `tertiary` |

The Barlow and Barlow Condensed fonts are embedded in the binary (SIL Open Font License, see [`assets/fonts/OFL.txt`](../assets/fonts/OFL.txt)).

To regenerate it with the wallpaper, add a template to `~/.config/matugen/config.toml` that reuses a `colors.json` template:

```toml
[templates.zehll]
input_path = '~/.config/matugen/templates/colors.json'
output_path = '~/.config/zehll/theme.json'
```

## Requirements

- [matugen](https://github.com/InioX/matugen), for wallpaper-based colors
