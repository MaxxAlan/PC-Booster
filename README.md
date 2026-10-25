# PC-Booster

**PC-Booster** is a small Windows console app for a few user-controlled maintenance tasks. The menu and prompts are in Vietnamese.

## Features

- **Boost PC:** review open applications, optional background services, and the High Performance power plan; choose each change before it is applied. Closed applications are not reopened automatically.
- **Restore settings:** restore the original startup type and running state for services, plus the previous power plan, changed by Boost PC. The app saves this recovery data under `%LOCALAPPDATA%\PCBooster\restore-state.json` until it is restored.
- **Clean temporary files:** review and optionally delete items older than seven days in the current user's `%TEMP%` folder. Files that are locked or inaccessible are skipped.
- **View log:** show recent maintenance actions from the temporary log file.
- **Remove AI:** scan 64-bit and 32-bit uninstall entries, current-user Store/MSIX apps, Claude Code's native Windows CLI path, and recognized global npm, pnpm, and Bun coding-agent packages. Remove the selected app/package, then offer a separate list and confirmation for known per-user data/config folders. That data can include sign-ins, settings, chat history, plugins, and downloaded models.
- **Malware scan:** request a quick, full, or selected-path scan from Microsoft Defender and show Defender's status and recent detections. Detection depends on Defender and its security intelligence; the app does not claim that its own byte heuristics can prove a file is safe.
- **Resource report:** sample process CPU use over two seconds, report current process memory, and list the largest registered app sizes where Windows provides an estimate. The report never closes processes automatically.
- **Administrator access:** the executable requests elevation when started so Windows can apply service changes and run uninstallers that require administrator access.

The app does not edit the registry to “clean” it. Windows does not provide a safe general-purpose registry-cleaning operation, and the previous version only targeted a dummy key. Malware scans use the installed Defender engine instead of shipping another large signature database. No file contents are sent to an AI API. Remove AI checks registered Windows apps, current-user Store apps, Claude's native CLI install, and recognized global npm/pnpm/Bun packages; it does not recursively search every drive or inspect project-local `node_modules` folders. Temporary logs are written to `%TEMP%`; service recovery data is kept until restored.

## Build

The PowerShell source is `PerformanceBooster.ps1`. To create the Windows executable, run `Invoke-ps2exe` from the `ps2exe` PowerShell module with the `-requireAdmin` option. The checked-in executable is `PerformanceBooster_v2.3.exe`.

## English summary

PC-Booster provides user-confirmed app and optional service tuning, service recovery, age-limited temporary-file cleanup, a temporary action log, **Remove AI** for registered desktop and Store apps, Claude's native CLI install, and recognized global npm/pnpm/Bun coding-agent packages, Microsoft Defender scans, and a local CPU/RAM/app-size report.
