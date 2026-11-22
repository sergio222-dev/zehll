//! Audio spectrum from cava, drawn behind the player popover like ii's `WaveVisualizer`.

use std::process::Stdio;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

// cava's default `ascii_max_range`: each bar is printed as 0..=1000.
const MAX: f32 = 1000.0;
// ii's raw output config, at half its frame rate: each frame redraws the popover.
const CONFIG: &str = "[general]
mode = waves
framerate = 30
autosens = 1
bars = 50

[output]
method = raw
raw_target = /dev/stdout
data_format = ascii
channels = mono
mono_option = average

[smoothing]
noise_reduction = 20
";

/// Streams spectrum frames of 50 values in 0..=1 while subscribed; cava stops when the stream is dropped.
pub fn events() -> impl Stream<Item = Vec<f32>> {
    stream::channel(2, async |mut output| {
        let config = std::env::temp_dir().join("zehll-cava.conf");
        if let Err(err) = tokio::fs::write(&config, CONFIG).await {
            eprintln!("zehll: cannot write the cava config: {err}");
            return;
        }
        let child = Command::new("cava").arg("-p").arg(&config).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(err) => {
                eprintln!("zehll: cannot run cava: {err}");
                return;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if output.send(parse_frame(&line)).await.is_err() {
                return;
            }
        }
    })
}

/// One line of cava's raw ASCII output, `12;340;1000;`, scaled to 0..=1.
fn parse_frame(line: &str) -> Vec<f32> {
    line.split(';').filter_map(|value| value.trim().parse::<f32>().ok()).map(|value| (value / MAX).clamp(0.0, 1.0)).collect()
}

/// Moving average over `window` neighbours on each side, like ii, so the bars read as one wave.
pub fn smooth(points: &[f32], window: usize) -> Vec<f32> {
    let last = points.len().saturating_sub(1);
    (0..points.len())
        .map(|index| {
            let span = index.saturating_sub(window)..=(index + window).min(last);
            // Edges repeat the end value, as ii does, so the average keeps 2 * window + 1 samples.
            let missing = (2 * window + 1) - span.clone().count();
            let edge = if index < window { points[0] } else { points[last] };
            (points[span].iter().sum::<f32>() + edge * missing as f32) / (2 * window + 1) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_ascii_frames() {
        assert_eq!(parse_frame("0;500;1000;"), vec![0.0, 0.5, 1.0]);
        assert_eq!(parse_frame("1200;x;"), vec![1.0]);
        assert!(parse_frame("").is_empty());
    }

    fn close(actual: Vec<f32>, expected: &[f32]) -> bool {
        actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-6)
    }

    #[test]
    fn smoothing_averages_neighbours_and_repeats_edges() {
        assert!(close(smooth(&[0.0, 0.0, 0.9, 0.0, 0.0], 1), &[0.0, 0.3, 0.3, 0.3, 0.0]));
        assert!(close(smooth(&[0.6, 0.0, 0.0], 1), &[0.4, 0.2, 0.0]));
        assert!(smooth(&[], 2).is_empty());
    }
}
