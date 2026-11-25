//! Live Claude Code and Copilot CLI sessions beside the meetings pill, one tinted logo each.
//!
//! The sessions come from the state file that ai_alert's hooks write
//! (`$XDG_RUNTIME_DIR/ai_alert/state.json`); Zehll only reads it.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use iced::widget::{Row, Space, column, container, mouse_area, svg, text};
use iced::{Alignment, Color, Element, Length};

use crate::{BODY_BOLD, Bar, CONDENSED_BOLD, Message, Tooltip, panel_style};

pub const TOOLTIP_SIZE: (u32, u32) = (280, 56);
const ICON_SIZE: f32 = 18.0;
const SLOT_WIDTH: f32 = 24.0;

static CLAUDE: LazyLock<svg::Handle> =
    LazyLock::new(|| svg::Handle::from_memory(include_bytes!("../assets/agents/claude.svg").as_slice()));
static COPILOT: LazyLock<svg::Handle> =
    LazyLock::new(|| svg::Handle::from_memory(include_bytes!("../assets/agents/copilot.svg").as_slice()));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Copilot,
}

impl Agent {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "claude" => Some(Agent::Claude),
            "copilot" => Some(Agent::Copilot),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Copilot => "Copilot CLI",
        }
    }

    fn icon(self) -> svg::Handle {
        match self {
            Agent::Claude => CLAUDE.clone(),
            Agent::Copilot => COPILOT.clone(),
        }
    }
}

/// What a session is doing, as the hooks record it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Started, nothing asked yet.
    Idle,
    /// Thinking or running tools.
    Working,
    /// Needs a decision: a permission or a question.
    Attention,
    /// Finished, waiting for the next prompt.
    Waiting,
}

impl State {
    fn parse(name: &str) -> Self {
        match name {
            "working" => State::Working,
            "attention" => State::Attention,
            "waiting" => State::Waiting,
            _ => State::Idle,
        }
    }

    pub fn color(self) -> Color {
        match self {
            State::Idle => Color::from_rgb8(0x9e, 0x9e, 0x9e),
            State::Working => Color::from_rgb8(0xd9, 0x77, 0x57),
            State::Attention => Color::from_rgb8(0x4a, 0x90, 0xd9),
            State::Waiting => Color::from_rgb8(0x4c, 0xaf, 0x50),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            State::Idle => "IDLE",
            State::Working => "THINKING",
            State::Attention => "NEEDS YOUR INPUT",
            State::Waiting => "WAITING FOR YOU",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub agent: Agent,
    pub state: State,
    /// Base name of the session's working directory; may be empty.
    pub project: String,
    pid: Option<u32>,
    started: f64,
}

/// Parses the `{"sessions": {id: {...}}}` state file, oldest session first. `None` when the
/// text is not valid JSON: the hooks rewrite the file in place, so a read can catch it half written.
pub fn parse(text: &str) -> Option<Vec<Session>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut sessions: Vec<Session> = value
        .get("sessions")
        .and_then(|sessions| sessions.as_object())
        .into_iter()
        .flat_map(|sessions| sessions.values())
        .map(|session| Session {
            // Sessions recorded before ai_alert knew the agent were Claude Code's.
            agent: session.get("agent").and_then(|agent| agent.as_str()).and_then(Agent::parse).unwrap_or(Agent::Claude),
            state: State::parse(session.get("state").and_then(|state| state.as_str()).unwrap_or_default()),
            project: session.get("cwd").and_then(|cwd| cwd.as_str()).unwrap_or_default().to_owned(),
            pid: session.get("pid").and_then(|pid| pid.as_u64()).and_then(|pid| u32::try_from(pid).ok()),
            started: session.get("ts").and_then(|ts| ts.as_f64()).unwrap_or_default(),
        })
        .collect();
    sessions.sort_by(|a, b| a.started.total_cmp(&b.started));
    Some(sessions)
}

/// The live sessions, or `None` to keep the last ones when the file is mid-write. A session
/// whose agent process is gone (a terminal closed without `SessionEnd`) is left out.
pub fn load() -> Option<Vec<Session>> {
    load_from(&state_path(), Path::new("/proc"))
}

fn load_from(path: &Path, proc: &Path) -> Option<Vec<Session>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        // No hooks have run yet, or ai_alert is not installed.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    let mut sessions = parse(&text)?;
    sessions.retain(|session| session.pid.is_none_or(|pid| is_agent(proc, pid)));
    Some(sessions)
}

fn state_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("ai_alert/state.json")
}

/// Whether `pid` is still a `claude` or `copilot` process. Like ai_alert, checks argv[0] too:
/// Copilot's main thread is named `MainThread`.
fn is_agent(proc: &Path, pid: u32) -> bool {
    let dir = proc.join(pid.to_string());
    let comm = std::fs::read_to_string(dir.join("comm")).unwrap_or_default();
    if Agent::parse(comm.trim()).is_some() {
        return true;
    }
    let cmdline = std::fs::read(dir.join("cmdline")).unwrap_or_default();
    let argv0 = cmdline.split(|&byte| byte == 0).next().unwrap_or_default();
    let name = Path::new(std::str::from_utf8(argv0).unwrap_or_default()).file_name();
    name.and_then(|name| name.to_str()).and_then(Agent::parse).is_some()
}

/// One logo per session, tinted by its state; nothing when there are none.
pub fn bar_button(bar: &Bar) -> Option<Element<'_, Message>> {
    if bar.agents.is_empty() {
        return None;
    }
    let logos = bar.agents.iter().enumerate().map(|(index, session)| {
        let color = session.state.color();
        let logo = svg(session.agent.icon())
            .width(ICON_SIZE)
            .height(ICON_SIZE)
            .style(move |_, _| svg::Style { color: Some(color) });
        mouse_area(container(logo).center_x(SLOT_WIDTH).center_y(30))
            .on_enter(Message::ShowTooltip(Tooltip::Agent(index)))
            .on_exit(Message::HideTooltip)
            .into()
    });
    Some(Row::with_children(logos).spacing(2).align_y(Alignment::Center).into())
}

/// The session's project and `CLAUDE CODE · THINKING`, in the state's color.
pub fn tooltip(bar: &Bar, index: usize) -> Element<'_, Message> {
    let theme = bar.theme;
    let content: Element<'_, Message> = match bar.agents.get(index) {
        Some(session) => {
            let project = match session.project.is_empty() {
                true => session.agent.name(),
                false => session.project.as_str(),
            };
            let status = format!("{} · {}", session.agent.name().to_uppercase(), session.state.label());
            column![
                text(project).font(BODY_BOLD).size(13).wrapping(text::Wrapping::None),
                text(status).font(CONDENSED_BOLD).size(13).color(session.state.color()).wrapping(text::Wrapping::None),
            ]
            .spacing(2)
            .into()
        }
        None => Space::new().into(),
    };
    container(content)
        .padding([8, 12])
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| panel_style(theme))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = r#"{"sessions": {
        "b": {"id": "b", "agent": "copilot", "state": "attention", "cwd": "zehll", "pid": 20, "ts": 2.0},
        "a": {"id": "a", "agent": "claude", "state": "working", "cwd": "backend", "pid": 10, "ts": 1.0},
        "c": {"id": "c", "agent": null, "state": "bogus", "cwd": "", "pid": null, "ts": 3.0}
    }}"#;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zehll-agents-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_process(proc: &Path, pid: u32, comm: &str, argv0: &str) {
        let dir = proc.join(pid.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        std::fs::write(dir.join("cmdline"), format!("{argv0}\0--flag\0")).unwrap();
    }

    #[test]
    fn parses_sessions_oldest_first() {
        let sessions = parse(STATE).unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!((sessions[0].agent, sessions[0].state, sessions[0].project.as_str()), (Agent::Claude, State::Working, "backend"));
        assert_eq!((sessions[1].agent, sessions[1].state, sessions[1].project.as_str()), (Agent::Copilot, State::Attention, "zehll"));
        // Missing agent defaults to Claude Code, an unknown state to idle.
        assert_eq!((sessions[2].agent, sessions[2].state, sessions[2].pid), (Agent::Claude, State::Idle, None));
    }

    #[test]
    fn half_written_file_keeps_last_sessions() {
        assert_eq!(parse(""), None);
        assert_eq!(parse(r#"{"sessions": {"a": {"#), None);
        assert_eq!(parse("{}"), Some(Vec::new()));
    }

    #[test]
    fn missing_file_means_no_sessions() {
        let dir = scratch();
        assert_eq!(load_from(&dir.join("state.json"), &dir), Some(Vec::new()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn drops_sessions_whose_agent_died() {
        let dir = scratch();
        let proc = dir.join("proc");
        std::fs::write(dir.join("state.json"), STATE).unwrap();
        // pid 10 is a claude process; pid 20 was reused by a shell.
        fake_process(&proc, 10, "claude", "claude");
        fake_process(&proc, 20, "fish", "/usr/bin/fish");
        let sessions = load_from(&dir.join("state.json"), &proc).unwrap();
        assert_eq!(sessions.iter().map(|session| session.pid).collect::<Vec<_>>(), [Some(10), None]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copilot_is_recognized_by_argv0() {
        let dir = scratch();
        fake_process(&dir, 30, "MainThread", "/usr/lib/node_modules/.bin/copilot");
        assert!(is_agent(&dir, 30));
        assert!(!is_agent(&dir, 31));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn states_use_ai_alert_colors() {
        assert_eq!(State::Idle.color(), Color::from_rgb8(0x9e, 0x9e, 0x9e));
        assert_eq!(State::Working.color(), Color::from_rgb8(0xd9, 0x77, 0x57));
        assert_eq!(State::Attention.color(), Color::from_rgb8(0x4a, 0x90, 0xd9));
        assert_eq!(State::Waiting.color(), Color::from_rgb8(0x4c, 0xaf, 0x50));
    }
}
