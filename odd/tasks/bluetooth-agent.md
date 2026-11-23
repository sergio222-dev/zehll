# Bluetooth agent

Objective: make the Bluetooth sub-panel pair and connect devices reliably by registering a BlueZ agent in Zehll and chaining pair, trust and connect. Branch: `fix/bluetooth-agent` (from `main`).
Delivery: one commit. Test runner: `cargo test`.

## Specs

- S1 "estoy viendo que el panel lateral de bluetooth no conecta bien a mi dispositivo MI BT", then "implementalo" (register an agent that accepts authorization and just-works pairing; PAIR pairs, trusts and connects).

## Assumptions

- A1 No other agent runs (no blueman, bt-agent, bluetoothctl or ii), so Zehll's agent is the default one while it runs.
- A2 The agent uses the `NoInputNoOutput` capability: Zehll has no UI to type or show a PIN.
- A3 Only requests for a pairing started from the sub-panel are accepted (confirmation, authorization, legacy PIN `0000`). Service authorization is accepted for paired or trusted devices. Everything else is rejected, so nearby devices can't pair on their own.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1 | inline | done | `fix: register a Bluetooth agent and trust devices on pair` |

## Log

- L1 (2026-11-23) Diagnosis. bluetoothd logs since 2026-11-17 repeat "Authentication attempt without agent" followed by "avctp auth_cb() Access denied: org.bluez.Error.Rejected" and "Hands-Free Voice gateway rejected 4C:72:74:8F:CB:84": without an agent, BlueZ rejects the profiles an untrusted device opens (AVRCP, HFP). No agent process runs and `bluetooth.rs` registered none. PAIR only called `Device1.Pair`, leaving the device untrusted and disconnected.
- L2 T1 done: `src/bluetooth_agent.rs` serves `org.bluez.Agent1` on the system bus for Zehll's whole life, registers it with `NoInputNoOutput` and requests default, and registers again when `org.bluez` gets a new owner. Pure `answer` policy (A3); rejections are `org.bluez.Error.Rejected`. PAIR now marks the device as pairing (guard), pairs, sets Trusted and connects. Tests RED (4 failed, 204 passed; the guard test was written with its code) then GREEN (208). Clippy clean on touched files. Live: the release build started with no "cannot serve/register" errors (RegisterAgent and RequestDefaultAgent returned OK); the system bus policy forbids calling the agent from a shell, so a temporary test served it on the session bus: a stranger's AuthorizeService and RequestConfirmation got `org.bluez.Error.Rejected`, a pairing in progress got PIN `0000`, confirmation and service accepted. NOT tested: pairing and connecting the real MI BT (not in range of BlueZ now) and a device-initiated profile connection.
