# Agent indicator

Right of the meetings pill, Zehll shows one logo per live [Claude Code](https://claude.com/claude-code) or
[Copilot CLI](https://docs.github.com/en/copilot/concepts/agents/about-copilot-cli) session, colored by what the
session is doing. It takes no space when no session is open.

| Color  | Meaning                                 |
| ------ | --------------------------------------- |
| Grey   | Session started, nothing asked yet      |
| Orange | Thinking or running tools               |
| Blue   | Needs your input (permission, question) |
| Green  | Finished, waiting for your next prompt  |

Hover a logo for the session's project and status.

## Requirements

The sessions come from [ai_alert](https://github.com/sergio222-dev/agent_indicator_end-4)'s hooks: install it with
`python3 install.py` so Claude Code and Copilot CLI write their state to `$XDG_RUNTIME_DIR/ai_alert/state.json`.
Zehll only reads that file; ai_alert's Quickshell widget is not needed.

## How it works

- Zehll reads the state file once a second. The hooks rewrite it in place, so a half-written read keeps the
  previous sessions.
- A session whose `claude` or `copilot` process is gone (a terminal closed without `SessionEnd`) is hidden right
  away, like ai_alert's own pruning.
- Sessions already open when the hooks were installed show up only after they restart.
