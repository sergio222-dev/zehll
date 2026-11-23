# Media player

[← Back to the README](../README.md)

The button left of the date shows pause while the active player plays and play otherwise; it is hidden when no MPRIS player runs. When playback starts, flames burst out of its edges; when it stops, they burn out as red embers. The flames are a wgpu shader (`src/flame.wgsl`) and are not drawn with iced's software renderer. Like ii, the active player is the one playing, preferring the one already shown; `playerctld` is ignored because it mirrors the others. Hover it for the track and the player name; click it to open the player popover, and click outside it or press Escape to close it. Shuffle and repeat are disabled for players that do not support them. Live streams have no length to seek in: they show a wave that moves while playing and `LIVE`, without seeking or elapsed time. While the popover is open and the player is playing, `cava` draws the audio spectrum behind it, like ii.

The popover uses the layer namespaces `zehll-media` and `zehll-media-catcher` for layer rules.

## Requirements

- `curl`, for cover art that players link over HTTP (such as Spotify)
- [cava](https://github.com/karlstav/cava), for the audio spectrum behind the player popover
