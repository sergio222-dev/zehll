use std::path::PathBuf;

use tokio::process::Command;

const SHADER: &str = include_str!("../assets/anti-flashbang.glsl");

/// Turns the anti-flashbang screen shader on or off. These are runtime Hyprland options, so a
/// config reload clears them; Zehll reapplies the saved choice at startup.
pub async fn set(enabled: bool) -> bool {
    let path = shader_path();
    if enabled {
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, SHADER));
        if let Err(err) = written {
            eprintln!("zehll: cannot write {}: {err}", path.display());
            return false;
        }
    }
    // Damage tracking only on the monitor level, so the shader sees whole frames.
    let (shader, damage_tracking) = if enabled { (path.display().to_string(), 1) } else { (String::new(), 2) };
    let config = format!("hl.config({{ decoration = {{ screen_shader = {shader:?} }}, debug = {{ damage_tracking = {damage_tracking} }} }})");
    if let Err(err) = Command::new("hyprctl").args(["eval", &config]).status().await {
        eprintln!("zehll: cannot set the screen shader: {err}");
    }
    is_enabled().await
}

/// Whether Hyprland currently uses Zehll's shader.
pub async fn is_enabled() -> bool {
    let Ok(output) = Command::new("hyprctl").args(["getoption", "decoration:screen_shader"]).output().await else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).lines().next().and_then(|line| line.strip_prefix("str: ")) == Some(&*shader_path().display().to_string())
}

fn shader_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    runtime.join("zehll/anti-flashbang.glsl")
}
