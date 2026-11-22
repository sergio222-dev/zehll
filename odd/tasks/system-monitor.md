# System monitor

Objective: a memory and CPU indicator left of the media button, with a hover tooltip and a sub-panel on click (artboard SystemMonitor, "System monitor", of the Zehll Bar Design canvas). Branch: `feat/system-monitor` (from `main`).
Test runner: `cargo test`.

## Specs

- S1 "a la izquierda de el reproductor(play/pause) un indicador de uso de memoria y cpu" (reference image): memory % and CPU % in a pill left of the media button; red at 90 % or more.
- S2 "cuando se le hace click deberia desplegar un subpanel como en la tercera imagen": CPU, GPU, memory and swap rings; disks with bars; network graph with download and upload.
- S3 "en hover algo como en la cuarta imagen": tooltip with RAM, swap (used, free, total) and CPU.
- S4 "actualiza el diseño primero": artifact v18, new board SystemMonitor; approved ("aprovado, implementalo").
- Assumption (A1): `/proc` is read every 2 s (always, so the graph has history when the sub-panel opens); `df` and `nvidia-smi` run only while the sub-panel is open. CPU temperature from hwmon `k10temp`/`zenpower`/`coretemp`; the tooltip's CPU column adds temperature and clock to the reference's load.
- Assumption (A2): disks are `df` filesystems without tmpfs, devtmpfs, efivarfs and overlay, `/` first then by size, at most 3. The network graph follows the default route's interface, 60 samples (2 min), top rounded up to 1/2/5 × 10ⁿ KB/s.
- S5 "veo que tiene transparencia, no deberia tener transparencia": the tooltip and the sub-panel use the popover background at full opacity, like the quick settings sub-panels.
- Assumption (A3): the sub-panel opens centered under the bar like the player (760 x 456) and closes on a click outside or Escape; it closes the panel, calendar and player popovers, and they close it.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S4 | inline | done | artifact SystemMonitor.dc.html v18 (no commit), approved |
| T2 | S1, S2, S3, S5, A1–A3 | inline | done | `feat: add a system monitor to the bar` |

## Log

- L1 (2026-11-22) Request with four images (the first two identical); "actualiza el diseño primero"; v18 published; "aprovado, implementalo".
- L2 T2 done. `usage.rs`: parsers for `/proc/stat` (iowait counts as idle), `/proc/meminfo` (used = total − available), `/proc/net/dev`, `/proc/net/route`, `/proc/cpuinfo`, `df -B1`, `nvidia-smi` CSV (MiB); `Usage::update` keeps CPU deltas and a 60-sample rate history; `details()` runs `df` and `nvidia-smi` with a 3 s timeout. `sysmon.rs`: pill (glow while hovered or open), three-column tooltip (460 x 122, under the pointer), sub-panel with canvas rings (270°, ticks, red at 90 %) and the network graph (newest on the right). `main.rs`: `UsageTick` every 2 s, `UsageDetails`, `ToggleSystem`, `Surface::System`, `Tooltip::System`, `open_system`/`close_system` wired into the other popovers, the catcher, Escape and `WindowClosed`. RED 7/7 (usage), then 1 more test for the graph scale written with its code; GREEN `cargo test` 159/159; clippy clean on the touched files. Live (Zehll was not running; started and stopped; ydotool clicks, cursor moved): pill showed 38 % / 11 %; tooltip 460 x 122 under the pointer with RAM 12.0/19.2/31.2 GiB, swap 0/4/4 GiB, CPU 9 %, 52 °C, 3.71 GHz; click opened `zehll-system` 760 x 456 centered on DP-1 with the catcher; rings CPU 5 %, GPU 7 % 49 °C 1.22/8 GiB, memory 12.0 of 31.2 GiB; disks `/`, `/home`, `/boot` (order fixed after the first run listed `/boot` second); Escape closed it. The graph's spikes are real: `/proc/net/dev` showed ~4.7 MB bursts every few seconds. Not tested: a machine without nvidia-smi (GPU ring shows `—`), the red state at 90 %.
- L3 S5: `sysmon::opaque` (popover background with alpha 1, same border). Live: the sub-panel's background sampled #090e20 in empty areas; the tooltip shows no wallpaper through. GREEN 159/159.
