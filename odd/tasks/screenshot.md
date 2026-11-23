# Screenshot region selector

Objective: a screen snip with a zone selector in the style of ii's region selector (`~/.config/quickshell/ii/modules/ii/regionSelector`), designed first in the Zehll Bar Design canvas (artboard Screenshot, "Screenshot region selector"). Branch: `feat/screenshot` (from `main`).
Delivery: stacked PRs to main, one per task (T2–T5), each based on the previous branch. Test runner: `cargo test`.

## Specs

- S1 "se quiere implementar el screenshot al estilo de II ~/.config/quickshell/ii/ , es posible implementarlo? con el selector de zonas ?": freeze the screen, select a window, a layer or a dragged rectangle, act on the crop.
- S2 Scope agreed ("dale"): rectangle drag, click on a highlighted window or layer, Escape cancels, left button copies (and saves when SAVE is on), right button edits in swappy. OCR, image search, circle selection, content regions and recording are out of scope.
- S3 Design in the artifact first: v19, new board Screenshot; approved ("se ve bien puedes implementarlo").
- Assumption (A1): the global shortcut is `zehll:screenshot`; pressing it while the selector is open cancels. Binding it (Super+Shift+S, today taken by ii) is a Hyprland config follow-up.
- Assumption (A2): each monitor is captured with `grim -o <name> -t ppm -` before any surface opens; one `zehll-snip` overlay per monitor shows its frozen frame. A selection stays on one monitor.
- Assumption (A3): targets are the top-level layers of the monitor that are not bars, docks or Zehll's own, then floating windows, then tiled windows of the monitor's active workspace (ii's order). A press and release closer than 4 px is a click.
- Assumption (A4): SAVE writes `$(xdg-user-dir PICTURES)/Screenshots/screenshot-YYYY-MM-DD_HH.MM.SS.png` (ii's name) and persists in Zehll's state file. The toolbar shows only on the focused monitor.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S3 | inline | done | artifact Screenshot.dc.html v19 (no commit), approved |
| T2 | S1, A3 | inline | done | `feat: add the snip selection model` |
| T3 | S1, A2 | inline | done | `feat: capture monitors and deliver snips` |
| T4 | S1, S2, A1–A3 | inline | done | `feat: add the snip zone selector` |
| T5 | S2, A4 | inline | done | `feat: add the snip toolbar with a save toggle` |

## Log

- L1 (2026-11-23) Question about feasibility; scope proposed and accepted ("dale"); v19 published; "se ve bien puedes implementarlo".
- L2 T2–T5, written in one go and live-tested together, then split (L3). `snip.rs`: pure `parse_monitors`, `regions` (layers of level 2 without bars/docks/Zehll and under 8 px, then floating windows by `focusHistoryID`, then tiled ones of the active workspace, clipped to the monitor), `selection`, `is_click`, `target`, `crop` (logical to physical, clamped), `size_label`, `file_name`; `capture` (one `grim -o <name> -t ppm -` per monitor, decoded with `image`), `deliver` (crop, PNG, `wl-copy --type image/png` plus save, or `swappy -f -`); a canvas keeps the drag locally and publishes only the release. `main.rs`: `ToggleSnip` (closes every popover, waits 150 ms when one was open, then captures), `open_snip` (one `zehll-snip` overlay per monitor, exclusive keyboard on the focused one), `close_snip`, `Snip`, `CancelSnip`, `ToggleSnipSave` (persisted as `snip_save`); `hyprland::layout`; shortcut `zehll:screenshot`; `image` 0.25 (png, pnm) as a direct dependency, already in the lock file through iced. RED 9/9, GREEN `cargo test` 168/168; clippy clean on the touched files. Live (Zehll was not running; started and stopped; `hyprctl dispatch global`, cursor warps and ydotool clicks): both monitors got a `zehll-snip` layer at full size; hovering Firefox showed the glow fill and tag, the COPY chip and the toolbar on the focused monitor only; a 500 x 300 drag showed the undimmed zone, accent brackets and `500 × 300`, and the clipboard PNG matched the frozen frame pixel for pixel (mean diff 0.0001); a click on the HDMI window copied it at its current size (1900 x 984); Escape closed both surfaces; a right-button drag opened swappy (closed afterwards); SAVE toggled and persisted, and with it on a 240 x 120 drag landed in `~/Pictures/Screenshots` and the clipboard (test file deleted, `snip_save` reset to false). Not reproduced: twice in early runs a click on SAVE right after a cross-monitor cursor warp reached the canvas instead of the button (the fullscreen Firefox got copied); 7 later tries, with logging, hit the button every time. Not tested: fractional scale and rotated monitors (crop assumes `width / scale` and no transform), a layer rule for `zehll-snip` animations, the Super+Shift+S binding (still ii's).
- L3 "dividilos en PR apilados": the 850-line change was split into four slices by scripted reverse edits of the live-tested files: T2 the pure model and its tests (`feat/screenshot-model`), T3 `capture`/`deliver`, `hyprland::layout` and the `image` dependency (`feat/screenshot-capture`), T4 the overlay, the selector surfaces and the shortcut, copying without saving (`feat/screenshot-selector`), T5 the toolbar, SAVE and its state, README and this document (`feat/screenshot-toolbar`). Each slice builds and passes `cargo test` on its own; T2 and T3 add code that is only used from T4 on, so they build with dead-code warnings. The last slice equals the live-tested files plus the README section (`cmp`).
