# Wallpapers

[← Back to the README](../README.md)

Zehll draws the desktop wallpaper itself and recolors the desktop with matugen, like ii's background.

## The wallpaper

Each monitor gets a layer below every window (namespace `zehll-wallpaper`) showing the wallpaper zoomed to 107 % of what covers it. Its left edge is shown on the monitor's first workspace and its right edge on the fourth, centered vertically; changing workspace slides it to its new place in 600 ms, easing out, like ii's parallax.

The wallpaper is scaled once per monitor size into `~/.cache/zehll/wallpaper`, so the picture kept in the GPU is no larger than the monitor needs. ii's background layer is drawn above Zehll's, so with Zehll it should be off.

The wallpaper and its accent are kept in `~/.local/state/zehll/wallpapers.json`, and the wallpaper comes back when Zehll starts.

## The Wallpapers page

Open the launcher and pick **Wallpapers** in Home, or use the global shortcut `zehll:wallpapers`, which opens the launcher on this page (and closes it when it is there). `zehll:wallpaperRandom` applies a random wallpaper from the folder without opening anything. ii binds the same actions to Ctrl+Super+T and Ctrl+Super+Alt+T; with a Lua config:

```lua
hl.unbind("CTRL + SUPER + T")
hl.unbind("CTRL + SUPER + ALT + T")
hl.bind("CTRL + SUPER + T", hl.dsp.global("zehll:wallpapers"), { description = "Shell: Change wallpaper" })
hl.bind("CTRL + SUPER + ALT + T", hl.dsp.global("zehll:wallpaperRandom"), { description = "Shell: Random wallpaper" })
```

 The launcher grows to a grid of the images in `$(xdg-user-dir PICTURES)/Wallpapers`, newest first; the search filters them by name. Thumbnails are made in the background and kept in `~/.cache/zehll/thumbs`.

| Key | Action |
|---|---|
| Arrows | Move among the wallpapers |
| Enter, or a click on the selected one | Apply it |
| Alt+Left / Alt+Right | Pick the accent color |
| Ctrl+R, or the shuffle button | Apply a random one |
| Shift+Escape | Back to Home |

The ACCENT strip shows the colors matugen can build the theme from (`--show-source-colors`); applying runs `matugen image <wallpaper> --mode dark --source-color-index <accent>`, which also rewrites [Zehll's theme](theme.md). The folder button opens the wallpaper folder.

## The Wallhaven page

Pick **Wallhaven** in Home, or use the global shortcut `zehll:wallhaven` (it toggles the launcher on this page), to search [wallhaven.cc](https://wallhaven.cc). For example, on Ctrl+Super+Shift+T:

```lua
hl.bind("CTRL + SUPER + SHIFT + T", hl.dsp.global("zehll:wallhaven"), { description = "Shell: Search Wallhaven" })
```

The search runs 600 ms after typing stops; scrolling or moving near the end loads the next page of 24.

- The filter bar toggles the categories (General, Anime, People) and the purity (SFW, Sketchy, NSFW), cycles the sorting (date added, relevance, random, views, favorites, toplist of the last month), flips the order and searches again. At least one category and one purity stay on.
- Sketchy and NSFW need an API key from your wallhaven account settings. The key button turns the search field into a hidden key field: Enter saves it, an empty field removes it, Shift+Escape cancels.
- Enter (or a click on the selected result) downloads the full image to `~/.cache/zehll/wallhaven/full` and applies it; Ctrl+Enter downloads it to the wallpaper folder as `wallhaven-<id>.<ext>`, so it stays in the Wallpapers page. A failed thumbnail retries on click.

The filters and the key are kept in `~/.local/state/zehll/wallpapers.json` under `wallhaven`; thumbnails are cached in `~/.cache/zehll/wallhaven/thumbs`.

## Requirements

- matugen
- curl, for Wallhaven
- `xdg-user-dir` to find the Pictures folder
