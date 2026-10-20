# Current delivery state — 2026-10-19

Implemented and connected: job CRUD with protected soft deletion, local/mounted
share selection, preview and approval, new/changed/unchanged execution, archive,
restore, expiry, history, native browsing, RU/EN and OS schedule registration.
Windows headless scheduling and next actual OS run-time query passed a disposable
live test. The requested launch form is an ordinary folder, not ZIP/installer.

Pending release gates: real NAS/removable disconnects, comprehensive path-swap and
power-loss/fault campaigns, multi-job trigger bursts and all performance scenarios,
Windows scaling/native picker end-to-end acceptance, Linux/macOS. No release-ready
claim follows from removing the UI's old development banner. Keep real errors and
warnings visible. The prior staged plan is retained below for history.

# Delivery plan

Work as one agent by default; implement small slices and run focused tests.

| Stage | Deliverable | Current state |
| --- | --- | --- |
| 1 | Workspace, boundaries, safety/IPC/storage design, reference, actual graph | Implemented scaffold |
| 2 | Compiled domain + SQLite repository + complete-scan dry-run planner | Domain/storage checks passed; initialization/language repository implemented; planner pending |
| 3 | Copy, archive, restore, conservative retention and recovery | Pending deterministic fault tests before enablement |
| 4 | Tauri + React/TS/Tailwind/shadcn, Dark Minimal, RU/EN | Development shell and status/language IPC implemented; backup UI pending |
| 5 | OS scheduler + headless CLI + locks and missed runs | Boundary only; no OS registration |
| 6 | Integration, platform and resource regression gates | 8 Rust unit tests and 7 SQL tests passed; runtime/performance gates pending |
| 7 | Build, package, dependency/license review, release | Windows x64 release-mode shell and NSIS installer built; full product/release gates pending |

Cargo/rustc are now available and the domain/storage tests passed on Windows.
Next: implement the complete-scan planner and live path/identity checks before
enabling destructive filesystem operations.
Do not substitute a Python engine or a fake web dashboard for the required stack.

Scheduling design: daily local wall time per job. OS adapter chooses the next actual
instant, with explicit DST rules: run once on repeated times; skipped times coalesce
to the next available opportunity only under run-once policy. OS registration and
preview-consent persistence must have recoverable pending/error states. Do not show
“enabled” until registration is verified. Windows is first; Linux must detect systemd
user availability and report unsupported/fallback setup instead of assuming it exists.
macOS uses launchd. No daemon or sleep inhibition by default. Logout, credentials,
mounted share availability and permissions are part of platform acceptance tests.

Performance gates before release: idle, 10k small files, large file, many configured
and runnable jobs, disconnected NAS, GUI-closed scheduled run. Record OS/hardware,
file count/bytes, network, elapsed time, peak RAM, idle/active CPU, throughput, UI
responsiveness and retries. No measurements or numeric budgets are claimed yet.
