# PC-Booster

**PC-Booster** is a Windows maintenance app that finds what slows a PC down (start-up
entries, optional services, temporary files, AI apps and disk usage), lets the user
confirm every change, and can undo everything it touched.

The application is a **Tauri desktop app** (Rust core + system WebView2). The same Rust
core is also available as a command line tool (`pcbooster.exe`) for scripting and for the
end to end tests.

## Features

- **Overview / health score** – RAM, drive free space, start-up count, old temp files and
  the AI apps found on the machine, summarised in one score with plain language notes.
- **Start-up manager** – lists every `Run`/`RunOnce` entry and both Start Menu start-up
  folders, and can disable them. Every change is journalled so it can be restored.
- **Optional services** – shows what each candidate service actually does before it is
  disabled. Only services that are genuinely unused are recommended; `SysMain`, `WSearch`
  and `XblGameSave` are listed as *not recommended* because disabling them usually makes
  the machine worse, not better.
- **Power plan** – switches to the built-in *High performance* plan and creates it when
  Windows hides it. The `powercfg` output is parsed by GUID, so it works on localized
  Windows (English, Vietnamese, …).
- **Temp cleanup** – lists items in the user temp folder older than seven days with their
  **real** recursive size, and moves them to the **Recycle Bin** instead of deleting them
  permanently.
- **AI app removal** – finds AI desktop apps, Store/MSIX apps, Claude Code's native CLI
  and global npm/pnpm/Bun coding agents. Uninstalling and deleting the leftover data
  folders are two separate, separately confirmed actions.
- **Microsoft Defender** – shows the engine status and signature age, starts quick, full
  or custom scans through `MpCmdRun.exe`, and lists recent detections.
- **Disk analysis** – free space per drive and the largest folders in the user profile.
- **Restore** – reverts the services, start-up entries, power plan and app removals that
  the app recorded in `%LOCALAPPDATA%\PCBooster\restore-state.json`.

The app does not touch the registry to "clean" it, does not ship its own signature
database, and sends nothing to an AI API.

## Architecture

```
crates/pcbooster-core   Rust engine: registry, services, power, temp, AI apps, Defender,
                        disk, journal. No GUI dependencies, unit tested.
crates/pcbooster-app    Tauri desktop app (Rust commands + plain HTML/CSS/JS UI).
bench/bench.ps1         PowerShell baseline used for the benchmark comparison.
legacy/                 The previous PowerShell console version, kept for reference.
scripts/build.ps1       Build + bundle.
scripts/test-e2e.ps1    End to end test suite.
```

### Why Rust instead of PowerShell

Both implementations were benchmarked on the same machine (`bench/bench.ps1` versus
`crates/pcbooster-core/src/bench.rs`). The numbers are in
[docs/benchmark.md](docs/benchmark.md).

The short version: the Rust core is roughly two orders of magnitude faster on the
operations that run on start-up, it is a single static binary with no PowerShell
start-up cost, and it is the only option that Tauri can package. The PowerShell
version stays useful as a fallback for machines where the Rust binary cannot run, and
as the baseline for the benchmark.

## Requirements

- Windows 10/11 x64
- WebView2 runtime (preinstalled on current Windows 10/11)
- Administrator rights only for the actions that change services or the power plan.
  The app starts as a normal user and asks for elevation per action.

## Build

```powershell
# toolchain
rustup toolchain install stable-msvc
cargo install tauri-cli --version "^2"

# build + NSIS installer
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1

# or directly
cargo tauri build
```

Artifacts:

- `target/release/pcbooster-app.exe` – the desktop application
- `target/release/pcbooster.exe` – the command line tool
- `target/release/bundle/nsis/*.exe` – the installer

## Command line

```
pcbooster overview      # health summary
pcbooster apps          # installed programs
pcbooster ai            # AI apps and coding agents
pcbooster startup       # auto-start entries
pcbooster services      # optional services with their impact
pcbooster temp          # temp files older than 7 days
pcbooster processes     # running processes
pcbooster disk          # drive usage
pcbooster power         # power schemes
pcbooster defender      # Defender status
pcbooster selftest      # verify every subsystem
pcbooster bench [n]     # measure the engine
```

Add `--json` to any command for machine readable output.

## Test

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-e2e.ps1
```

The suite runs the Rust unit tests, every CLI command, the packaged desktop binary in
self test mode, a window start-up check, the bundle check and the benchmark comparison.

## Changes in 2.4 compared to 2.3

| Problem in 2.3 | Fix in 2.4 |
|---|---|
| Source code was not in the repository, only the compiled `.exe` | Full Rust source in `crates/`, build and test scripts included |
| `Get-ItemProperty` threw `InvalidCastException` on malformed uninstall keys and printed a raw error | Typed registry reads, broken entries are skipped and logged |
| `powercfg` parsing matched the English label, so it silently failed on Vietnamese Windows | Parsing by GUID, locale independent |
| The executable always demanded administrator rights | Runs as a normal user, elevation is requested per action |
| The app could delete its own log because the log lived in `%TEMP%` | Log and journal live in `%LOCALAPPDATA%\PCBooster` |
| The temp cleanup reported a wrong size (directories have no length) | Recursive size calculation |
| Temp files were deleted permanently | Moved to the Recycle Bin |
| Closing an application killed it immediately | The window is asked to close first, force kill is the fallback |
| Recommended services (`SysMain`, `WSearch`, `XblGameSave`) usually made the PC worse | Each service shows its real impact, only unused ones are recommended |
| No start-up management, no health score, no disk analysis | New modules plus a Tauri UI |
| Defender status and scans went through PowerShell cmdlets | Scans run through `MpCmdRun.exe`, status only where it is needed |
| Elevated runs could act on the wrong user profile | The interactive user profile is resolved from the session |

## Legacy PowerShell version

`legacy/PerformanceBooster.ps1` is the 2.3 console version with the critical bugs of
2.3 fixed (registry crash, localized `powercfg`, log location, elevation model). It is
kept for reference and as the benchmark baseline; the desktop app is the supported
product.

## License

MIT
