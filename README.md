# macfan

**A fast, safe, dependency-light terminal UI for controlling your Mac's fans.**

macfan talks directly to the SMC (System Management Controller) through IOKit — no kernel extensions, no background daemons, no Electron. One Rust binary that shows live fan RPM and temperatures, lets you pin fan speeds with a couple of keystrokes, and *guarantees* your fans go back to automatic control when it exits.

Built and tested on an Apple Silicon MacBook Pro (M3 Pro, `Mac15,7`) running macOS 26, including the M3/M4-generation thermal-manager unlock that most older fan tools don't handle.

```
 MACFAN  Mac15,7 · Apple M3 Pro   CONTROL
┏━ ▶ Left Fan  MANUAL  linked ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃  3120 RPM  target  3500        min 1350 · max 5349                      ┃
┃ ████████████████████▌░░░░░░░░░░░░░░░ 3120 RPM ░░░░░░░░░░░░░░░░░░░░░░░░  ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
╭─   Right Fan  MANUAL ───────────────────────────────────────────────────╮
│  3354 RPM  target  3500        min 1458 · max 5777                      │
│ █████████████████████░░░░░░░░░░░░░░░ 3354 RPM ░░░░░░░░░░░░░░░░░░░░░░░░  │
╰─────────────────────────────────────────────────────────────────────────╯
╭─ thermals ───────────────────────────────────────────────────────────────╮
│ avg 47.7°C   hottest 79.0°C (TCMz)   215 sensors                         │
╰──────────────────────────────────────────────────────────────────────────╯
╭─ Left Fan — rpm history ─────────────────────────────────────────────────╮
│                                       ▂▄▆█████████████████████████       │
│ ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▆███████████████████████████       │
╰──────────────────────────────────────────────────────────────────────────╯
 fans restore to auto on quit (q) — use Q to keep settings
 ↑↓ select   ←→ ±100   ⇧←→ ±500   m manual/auto   a all auto   f full blast   space link   q quit·restore   Q quit·keep
```

## Features

- **Live dashboard** — per-fan RPM gauge, target, hardware min/max, and mode badge (`AUTO` / `MANUAL` / `CURVE` / `SYSTEM`), refreshed every second
- **Temperature-response fan curves** — define piecewise-linear `(hotspot °C, fan %)` curves on the command line or interactively in the TUI, continuously evaluated every second
- **Full thermal picture** — auto-discovers every readable temperature sensor on your machine (215 on an M3 Pro) and shows the average and the hottest one, so you can see *why* the fans are spinning
- **RPM history sparkline** — a rolling 4-minute graph for the selected fan
- **Linked or independent control** — adjust both fans together (default, matches how MacBook thermals are designed) or each one separately
- **M3/M4 thermal-manager unlock** — handles the `Ftst` diagnostic unlock that newer Apple Silicon firmware requires before it accepts manual fan control (the same sequence used by [Stats](https://github.com/exelban/stats))
- **Sleep/wake resilient** — if macOS reclaims fan control (it does, after sleep/wake), macfan detects it within ~2 seconds and re-engages your setting
- **Safe by construction** — fans are returned to automatic control on quit, on panic, on crash of the UI, on `Ctrl-C` — on every exit path except an explicit "keep my settings" quit
- **Read-only mode** — run without `sudo` to just watch fans and temperatures
- **Zero runtime dependencies** — a single static binary; the only Rust dependency is [ratatui](https://github.com/ratatui/ratatui)

## Compatibility

| Hardware | Status |
|----------|--------|
| MacBook Pro M3 Pro (`Mac15,7`), macOS 26 | ✅ developed & tested on this machine |
| Other M3 / M4 Macs with fans | ✅ expected to work (same `Ftst` unlock path) |
| M1 / M2 Macs with fans | ✅ expected to work (direct mode write, no unlock needed) |
| M5 Macs | ⚠️ untested — macfan probes the lowercase `F0md` key M5 firmware uses, but no hardware was available to verify |
| Intel Macs | ⚠️ untested — the Intel paths (`fpe2` fixed-point encoding, `FS!` force bitmask, `F0ID` fan names) are implemented but have not been run on real hardware |
| Fanless Macs (MacBook Air) | ❌ nothing to control; macfan exits with a clear message |

Reading fan state and temperatures never requires privileges. **Writing** fan speeds requires root — that's enforced per-key by the SMC firmware itself, not by macfan.

## Install

You need a [Rust toolchain](https://rustup.rs) (1.85+, edition 2024).

```sh
git clone https://github.com/raminsharifi/macfan.git
cd macfan
cargo build --release
sudo ./target/release/macfan
```

Optionally install it on your `PATH`:

```sh
sudo cp target/release/macfan /usr/local/bin/
```

## Usage

```
macfan                     launch the TUI (sudo required to change speeds)
macfan --curve [SPEC]      launch the TUI with a temperature-response fan curve
macfan --list              print fans and temperatures, then exit
macfan --auto              restore all fans to automatic control, then exit
macfan --help              show help
macfan --version           print version, then exit
```

### Fan curves

Pass `-c` / `--curve` with coordinate pairs mapping hotspot temperature (°C) to fan speed (% of each fan's `min`–`max` RPM range). Between points, fan speed ramps piecewise-linearly; below the lowest point and above the highest point it clamps:

```sh
sudo macfan --curve 40:30,60:60,90:100
sudo macfan --curve "40c:30%, 60c:60%, 90c:100%"
sudo macfan --curve "(40c, 30%) -> (60c, 60%) -> (90c, 100%)"
sudo macfan --curve   # uses the default 40°C:30% → 60°C:60% → 90°C:100% curve
```

Inside the TUI you can also press `c` to toggle the curve on/off or `C` to edit the curve points inline.

`macfan --list` works without sudo:

```
Mac15,7 · Apple M3 Pro

  Left Fan         0 RPM  target     0  range 1350–5349  [SYSTEM]
  Right Fan        0 RPM  target     0  range 1458–5777  [SYSTEM]

  temps: avg 46.7°C, hottest 77.4°C (Tf26), 215 sensors
```

(Yes, 0 RPM is real — on M3-class machines the thermal manager turns the fans completely off at idle.)

### Keys

| Key | Action |
|-----|--------|
| `↑` `↓` / `k` `j` | select fan |
| `←` `→` / `h` `l` | target −/+ 100 RPM |
| `⇧←` `⇧→` / `H` `L` | target −/+ 500 RPM |
| `+` `-` | target +/− 100 RPM |
| `m` | toggle manual / automatic for the selected fan |
| `c` | toggle temperature-response fan curve mode |
| `C` | edit the fan curve inline (`temp:pct` points, `Enter` to apply, `Esc` to cancel) |
| mouse | on the **fan curve** plot: drag a point to move it (applied live while the curve is active), click empty space to add one, right-click a point to delete it, scroll over a point for ±1%; hover shows the point's °C, % and RPM |
| `u` / `D` | undo the last curve edit / reset the curve to the default |
| `a` | all fans back to automatic |
| `f` | full blast (pin at hardware max) |
| `space` | toggle linked mode (apply changes to all fans vs. just the selected one) |
| `r` | force an immediate refresh |
| `q` / `Esc` | quit **and restore automatic control** |
| `Q` | quit keeping your manual settings |

Adjusting the speed of a fan that's in automatic mode switches it to manual, starting from its current RPM. Targets are always clamped to the hardware-reported range (`F0Mn`–`F0Mx`).

> **Note:** the *first* time you engage manual control on an M3/M4 Mac, expect a 3–6 second delay while macfan unlocks fan control from the thermal manager (status line shows progress). Subsequent adjustments are instant. See [How it works](#how-it-works).

## How it works

### The SMC

Every Mac has an SMC that owns fans, temperature sensors, power rails, and hundreds of other keys, each addressed by a four-character code. macfan opens the `AppleSMC` IOKit service from userspace and speaks the same 80-byte struct protocol used by every fan tool since [smcFanControl](https://github.com/hholtmann/smcFanControl) — implemented here in ~300 lines of Rust FFI with no helper libraries.

The keys that matter for fans:

| Key | Type | Meaning |
|-----|------|---------|
| `FNum` | `ui8` | number of fans |
| `F0Ac` | `flt` | actual RPM (read-only) |
| `F0Tg` | `flt` | target RPM |
| `F0Mn` / `F0Mx` | `flt` | firmware-recommended min/max RPM |
| `F0Md` | `ui8` | fan mode: `0` auto · `1` manual · `3` system (thermal manager) |
| `Ftst` | `ui8` | thermal-manager unlock flag (M1–M4) |

On Apple Silicon all RPM values are little-endian IEEE-754 floats; on Intel they're big-endian 14.2 fixed-point (`fpe2`), and forcing manual mode uses the `FS!` bitmask instead of `F0Md`. macfan reads the type of each key at runtime and encodes accordingly, so both generations are handled by the same code.

### The M3/M4 unlock

On M1 Macs, writing `F0Md = 1` as root just works. From the M3 generation on, `thermalmonitord` holds the fans in mode `3` and the firmware rejects manual-mode writes with SMC error `0x82`. The working sequence — verified against the [Stats](https://github.com/exelban/stats) source and the [macos-smc-fan research](https://github.com/agoodkind/macos-smc-fan) — is:

1. Try `F0Md = 1` directly (works on M1, and on M3 when the system isn't actively asserting mode 3)
2. On rejection: write `Ftst = 1`, wait ~3 s for the thermal manager to yield, then retry the mode write (up to 300 × 100 ms)
3. Write the target RPM to `F0Tg`

macfan implements exactly this, then keeps watching: sleep/wake resets `Ftst` in firmware and the system reclaims the fans, so an idle loop in the control thread re-runs the sequence automatically whenever your desired state and the hardware state diverge.

### Architecture

```
main thread                      control thread
┌─────────────────────┐  Cmd →  ┌──────────────────────────────┐
│ ratatui UI loop     │ ────────│ owns its own SMC connection  │
│ read-only SMC conn  │ ← Note  │ unlock / retry / clamp       │
│ 1 Hz refresh        │ ────────│ re-assert after sleep/wake   │
│ key handling        │         │ restore-to-auto on Drop      │
└─────────────────────┘         └──────────────────────────────┘
```

All SMC writes happen on a dedicated thread so the 3–6 s unlock never freezes the UI. The two threads each hold their own IOKit connection and communicate over channels.

### Safety design

Manual fan control overrides macOS thermal management, so the failure modes were designed first:

- **Restore on every exit path.** The control thread restores automatic mode from a `Drop` guard — it runs on normal quit, on UI errors, and on panics in either thread. Only an explicit `Q` ("quit keeping settings") skips it.
- **`Ftst` is released conservatively.** The unlock flag is cleared only after every fan is back under automatic control, and only if macfan set it in the first place — leaving it stuck at `1` would partially inhibit macOS thermal management.
- **Intent is tracked eagerly.** The moment a mode write lands on the hardware, the fan is marked dirty — so even if the very next write fails, the exit-restore still covers it. A fan can never be silently stranded in manual mode.
- **Targets are clamped** to the firmware-reported `F0Mn`–`F0Mx` range, and `F0Mx`/`F0Mn` themselves are never written.
- **Quit is interruptible.** Pressing `q` mid-unlock aborts the retry loops within ~50 ms instead of queueing behind them.
- **External state is respected.** macfan only restores fans *it* touched — if another fan tool set something, exit leaves it alone.

If macfan is ever killed in a way nothing can survive (`kill -9`, power loss), run:

```sh
sudo macfan --auto
```

or just reboot — the SMC resets fan control on its own at startup.

## Troubleshooting

**"permission denied — restart with: sudo macfan"** — SMC writes require root. Reads don't, which is why the dashboard works without sudo.

**First manual adjustment takes several seconds** — that's the `Ftst` unlock on M3/M4 (see above). It only happens when the thermal manager holds the fans.

**Fans show `SYSTEM` and 0 RPM** — normal on M3+ at idle. The thermal manager turns fans fully off when the machine is cool.

**"could not enable manual mode: rejected by thermal manager (SMC 0x82)"** after the unlock retries are exhausted — the thermal manager refused to yield (heavy thermal load can cause this). Try again, or accept that macOS really wants those fans under its control right now.

**Settings revert after sleep** — expected; firmware resets the unlock on wake. macfan re-engages your target automatically within a couple of seconds while it's running.

**My Mac isn't in the compatibility table** — `macfan --list` is always safe to try (it's read-only). If it shows your fans correctly, control will likely work too. Issues and reports from other models are very welcome.

## Development

```sh
cargo build          # debug build
cargo test           # unit tests (struct layout, fourcc, fpe2/flt codecs)
cargo clippy         # lint-clean
```

| Module | Purpose |
|--------|---------|
| `src/smc.rs` | IOKit FFI, 80-byte SMC protocol, typed key read/write, key enumeration |
| `src/fan.rs` | fan discovery, mode read/write, RPM encoding (`flt`/`fpe2`), Intel fallbacks |
| `src/control.rs` | control thread: unlock sequence, retries, re-assertion, restore guarantees |
| `src/persist.rs` | saves the edited curve to `~/.config/macfan/curve` (owned by the invoking user under `sudo`) and reloads it on start |
| `src/plot.rs` | maps curve coordinates to terminal cells for the plot and mouse editing |
| `src/curve.rs` | piecewise-linear temperature-response fan curve parser and evaluator |
| `src/temps.rs` | temperature sensor discovery and polling |
| `src/app.rs` | TUI state and key handling |
| `src/ui.rs` | ratatui rendering |

The SMC struct layout is locked by a compile-time size assertion and the byte codecs are unit-tested; everything hardware-facing was verified against a live probe of the SMC on real M3 hardware.

## Disclaimer

macfan deliberately overrides macOS thermal management. Pinning fans *below* what the system wants under load means more heat; modern Macs will throttle and ultimately power off before damaging themselves, but sustained high temperatures are still not what you want. Keep an eye on the thermals panel, prefer `q` (restore on quit) over `Q`, and use this software at your own risk — see [LICENSE](LICENSE).

## Credits

macfan stands on prior work that mapped this territory:

- [exelban/stats](https://github.com/exelban/stats) — the reference implementation of the M3/M4 `Ftst` unlock sequence
- [agoodkind/macos-smc-fan](https://github.com/agoodkind/macos-smc-fan) — research documenting Apple Silicon fan-control behavior generation by generation
- [hholtmann/smcFanControl](https://github.com/hholtmann/smcFanControl) — the canonical open-source SMC protocol implementation
- [beltex/SMCKit](https://github.com/beltex/SMCKit) and [narugit/smctemp](https://github.com/narugit/smctemp) — additional protocol references
- [ratatui](https://github.com/ratatui/ratatui) — the TUI framework

## License

[MIT](LICENSE)
