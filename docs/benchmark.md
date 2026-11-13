# Benchmark: PowerShell vs Rust

Both implementations run the same operations on the same machine, so the numbers are
directly comparable.

**Machine** – Windows 11 Pro (build 26100), Intel CPU, 24 GB RAM, 252 registered programs,
198 Store packages, 17 start-up entries.
**Date** – 2026-11-13.
**Method** – one warm-up call, then 5 measured iterations per operation, median reported.
PowerShell 5.1.26100.9444 versus Rust 1.97.1 (`--release`, `lto = "thin"`).

Reproduce with:

```powershell
.\target\release\pcbench.exe 5                 # Rust
powershell -File bench\bench.ps1 -Iterations 5  # PowerShell
```

## Results (median milliseconds)

| Operation | PowerShell | Rust | Speed-up |
|---|---:|---:|---:|
| `registry.installed_apps` – read 252 uninstall entries | 201.17 | 13.48 | **14.9x** |
| `registry.store_packages` – 198 MSIX packages | 136.61 | 6.01 | **22.7x** |
| `registry.startup_entries` – 17 auto-start items | 14.57 | 0.32 | **45.5x** |
| `processes.list` – processes with window titles | 21.94 | 28.97 | 0.8x |
| `temp.scan` – temp items older than 7 days | 12.38 | 0.38 | **33.0x** |
| `services.query` – 4 optional services | 2.94 | 1.26 | **2.3x** |
| `power.list` – parse `powercfg /list` | 12.69 | 9.94 | **1.3x** |
| `ai.scan` – AI apps in registry, Store and npm | 1816.24 | 24.13 | **78.9x** |
| `overview.full` – the whole start-up scan | 505.56 | 54.40 | **9.3x** |
| **Total of all operations** | **2645.0** | **136.8** | **19.3x** |

## Reading the numbers

- **Registry work dominates the PowerShell side.** `Get-ItemProperty` builds a property bag
  per key and casts every value; the Rust core reads each value with an explicit type
  check. That is also why the PowerShell version could crash with
  `InvalidCastException` on a malformed uninstall key while the Rust version just skips it.
- **`ai.scan` is the worst case for PowerShell** (1.8 s): `npm list --global --json` is a
  Node process, and PowerShell pays for the pipeline plus JSON parsing on top of it. The
  Rust core parses the same JSON with `serde_json`.
- **`processes.list` is a tie.** The Rust version does more work (it resolves the image
  path, CPU time and working set of every process, not just the windowed ones). It pays
  for the extra data by opening a handle per process; PowerShell gets the data from a
  cached `Get-Process` snapshot.
- **`power.list` is close** because both implementations spawn `powercfg` and wait for
  it. The saving in Rust is only in the parsing (and Rust parses it locale-proof, which
  PowerShell could not do).
- **Start-up cost of the runtime itself.** A `powershell.exe` process needs roughly
  300–600 ms before the first line of useful work, while the Rust binary is ready in a
  few milliseconds. That difference is not visible in the table above because both
  benchmarks measure the operations only, but it is what the user feels when the app
  opens.

## Decision

**Rust wins on every axis that matters for this application:**

1. **Speed** – 19x faster overall, 75x on the AI scan that runs when the app opens.
2. **Packaging** – Tauri requires a Rust backend; the PowerShell version could only be
   shipped as a console app (ps2exe) or wrapped in a shell.
3. **Correctness** – typed registry access, locale independent `powercfg` parsing and
   real file sizes instead of the PowerShell approximations.
4. **Distribution** – one static binary, no PowerShell 5.1 dependency on the target
   machine (PowerShell 5.1 is still present on Windows 11, but it is a deprecated
   component and the Windows App SDK / Store policy keeps pushing replacements).

The PowerShell version is kept in `legacy/` for two reasons: it is the fallback for
environments where the Rust binary is blocked by policy, and it is the baseline used by
`bench/bench.ps1` so that future changes can be measured against it.
