# Bluetooth stale bond

Objective: when a paired device no longer has this computer's link key (it paired with another host), CONNECT forgets the stale bond and pairs the device again instead of failing. Branch: `fix/bluetooth-key-missing` (from `main`).
Delivery: one commit. Test runner: `cargo test`.

## Specs

- S1 "cuando tengo un bluetooth que ya fue paireado, pero luego se paireo con otro dispositivo y vuelve a iniciar el sistema, zehll me dice que el dispositivo ya esta paired, pero al dar connect no se conecta (se desconecta al poco tiempo)", then "el re-pair automatico me gusta".

## Assumptions

- A1 `br-connection-key-missing` / `le-connection-key-missing` (BlueZ `src/error.h`) only mean the remote lost the link key, so dropping the local bond loses nothing that still works.
- A2 The device must be in pairing mode to be found again; Zehll waits 30 s for it, then gives up with an error in the log (the device stays forgotten, PAIR works as usual afterwards).
- A3 No UI change: the device disappears from the list during the wait and comes back paired, which is the progress shown.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1 | inline | done | `fix: re-pair Bluetooth devices that lost their link key` |

## Log

- L1 (2026-11-25) Diagnosis. Soundcore Life Q20 (C0:EA:C3:18:57:B8) shows Paired/Bonded/Trusted in `bluetoothctl info`, but `~/.local/state/zehll/zehll.log` has four `Connect ... failed: org.bluez.Error.Failed: br-connection-key-missing`, and bluetoothd logs `Hands-Free Voice gateway failed connect ... Connection refused (111)` then `No matching connection for device` two seconds later: the link opens, authentication fails because the headset dropped the key when it paired with another host, and BlueZ hangs up. `bluetooth.rs` CONNECT only called `Device1.Connect` and logged the error; the only way out was FORGET, scan, PAIR by hand.
- L2 T1 done: CONNECT matches the key-missing reason (pure `is_key_missing`), then `repair`: `RemoveDevice` on the device's adapter (pure `adapter_of`), `StartDiscovery` on its own connection, waits (signal-driven, 30 s cap) for the device object to come back, `StopDiscovery`, and runs the same pair, trust, connect chain as PAIR (extracted to `pair`). `read` now goes through a shared `managed_objects`. Tests RED (2 failed, 233 passed) then GREEN (235). Clippy clean on `bluetooth.rs`.
- L3 (2026-11-25) Live test with the Q20 in the stale state, release build of this branch in place of the user's instance. User put the headset in pairing mode and pressed CONNECT: Zehll logged "lost its pairing key; pairing again", then `bluetoothctl info` showed Paired, Bonded, Trusted and Connected, and bluetoothd logged the A2DP transport ready (`sep2/fd0: fd(31) ready`) with no further failure. NOT tested: the 30 s timeout path (device not in pairing mode) and LE devices.
