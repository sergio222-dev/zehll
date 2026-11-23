# Wallpapers

Objective: Zehll controls the desktop wallpaper like ii (wallpaper per monitor, workspace parallax, colors from the wallpaper) and adds a wallpaper browser in the launcher modelled on bnsplits/dotfiles' Wallhaven page, designed first in the Zehll Bar Design canvas (artboard Wallpapers, "Wallpapers and Wallhaven"). Branches: `feat/wallpaper-core`, `feat/wallpaper-page`, `feat/wallhaven-page`, stacked to main.
Test runner: `cargo test`.

## Specs

- S1 "hoy en dia zehll no maneja backgrounds, queremos que se controle los backgrounds como hace II, y tambien agregar un buscador de wallpapers, el buscador tendra que basarse en como muestra la imagen (bnsplits/dotfiles)".
- S2 "podemos hacerlo con awww pero con los efectos parallax de II? parece que es mas optimo/rapido para la memoria"; answer after measuring: "awww con parallax aproximado: un recorte por workspace y fade o barrido al cambiar".
- S3 Design: artifact v20, new board Wallpapers; approved ("se ve bien puedes implementarlo").
- S4 "estoy viendo una pelicula si haces prueba trata de hacerlo en el monitor hdmi": live tests on HDMI-A-2 only.
- S5 After the awww version: "veo que el fondo si se desplaza pero hay un problema, no es una transicion, es algo instantaneo, si ese es el caso tal vez debamos desistir de esto e implementarlo como lo hace ii"; measured (awww wipes, cannot pan); answer: "avanza con eso".
- Assumption (A1): Zehll draws the wallpaper itself, like ii: one layer per monitor (namespace `zehll-wallpaper`, layer Background, all anchors, exclusive zone -1, no keyboard, input-transparent), opened with the monitor's bar. ii's own background must be off (it is drawn above).
- Assumption (A2): parallax as ii's: the wallpaper covers the monitor zoomed by 1.07, its left edge on slot 1 and right edge on slot 4, centered vertically; a slot change slides it with ii's 600 ms ease-out cubic, from where it is drawn. The wallpaper is scaled once per monitor size to a PNG in `~/.cache/zehll/wallpaper` (stable hash of path, size and mtime plus the scaled size), so the texture is no larger than the monitor needs.
- Assumption (A3): the wallpaper and its accent index persist in `~/.local/state/zehll/wallpapers.json`; at start the wallpaper is shown again without recoloring. A new wallpaper runs `matugen image <path> --mode dark --source-color-index <accent>`, which also writes Zehll's theme.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S3 | inline | done | artifact Wallpapers.dc.html v20 (no commit), approved |
| T2 | S1, S2, A1–A3 | inline | done | `feat: draw the wallpaper with awww and stepped parallax` |

## Log

- L1 (2026-11-23) Research: ii draws its own `quickshell:background` layer (Bottom) with workspace parallax (zoom 1.07), hides it under fullscreen, runs `switchwall.sh` (matugen) on change; bnsplits' Wallhaven page uses the public API (24 per page, categories, purity, sorting, toplist range, API key), streams thumbnails, applies with `awww img` and saves numbered copies. Measured awww: daemon ~3 MB RSS with `--transition-type none`, buffers 8.1 MB (1920x1080) and 14.4 MB (2560x1440) in shared memory, ~70 ms per `awww img` of a 2592x1728 PNG. awww cannot pan or zoom, so the parallax is stepped (S2).
- L2 T2. First version with awww: one crop per slot and monitor, handed to awww with a 0.25 s wipe; live, every slot showed its crop. The user saw it as instant (S5). Measured: awww's transition is a wipe that replaces one crop with the other, the 84 px apart crops read as a jump (4 frames at 0.25 s), and awww cannot pan (one `awww img` takes ~70 ms). Replaced by Zehll's own layer (A1, A2). `wallpaper.rs`: pure `State::{parse, merge_into}`, `cover`, `offset`, `Pan` (ease-out cubic), `cache_name` (FNV-1a); `prepare` (decodes once, only when a size is missing; Triangle resize + fast PNG, written atomically), `Layer`, `layer_settings`, `view` (a canvas drawing the image at the pan), `recolor`; `hyprland::monitors_json`; `image` gains jpeg, webp, bmp, gif (no new crates in the lock file). `main.rs`: `Surface::Wallpaper`, state restored at start, `ApplyWallpaper`, `WallpaperPrepared`, `WallpaperFrame` every 16 ms while a pan runs, `open_wallpaper` with each bar, slides from `set_monitors`, layers closed with their monitor. RED 3/5, GREEN `cargo test` 176/176; clippy clean on the touched files. Live (HDMI-A-2): two `zehll-wallpaper` layers at level 0 (2560x1440, 1920x1080); going from slot 3 to 4 slid the picture 0, 4, 7, 10, 13, 18 … 44, 45 px over ~500 ms (target 44.7). Memory, same binary, two runs: 357 MB RSS without a wallpaper, 444 MB with it (anon +1.5 MB, shared +42 MB for the two full-screen surfaces' buffers, file +43 MB for the GPU texture). Firefox was found out of fullscreen after this test, with Discord opened and focused on HDMI after the script returned the focus; not caused by an Escape (none sent), left as found.

