# First alpha distribution and Inno Setup 7 — 2026-10-20

Version is 0.1.0-alpha throughout Cargo/npm/Tauri and Inno's displayed version;
installer numeric Windows version is 0.1.0.0. Third-party package versions remain
locked; CLI adds only an existing tempfile dev dependency for cleanup safety tests.
Output: releases/0.1.0-alpha/portable-x64, portable-x86, setup-x64.iss,
setup-x86.iss, shared setup-common.iss, BUILD_INSTALLERS.md and installers/*.exe.

Actually executed on Windows 10 19045 x64 (x86 under WOW64):
- Production Tauri/TypeScript/Vite and release CLI builds passed for both targets.
  Portable packager verifies PE architecture and GUI ProductVersion before copy.
- cargo test --workspace --locked: 54 passed / 4 opt-in skipped per target.
  New CLI test checks missing state creates nothing, rejects relative state and
  active workers, disables schedule flags idempotently, and preserves files/history.
- cargo check, clippy -D warnings for both targets and fmt --check passed.
- Inno Setup 7.1.0 compiled both distribution installers; Russian and English
  included. Ordinary installation is per-user, no installer elevation required.
- scripts/test-installers.ps1: four isolated QA-identity installs/uninstalls
  (x64/ru, x64/en, x86/ru, x86/en) passed. For EACH, two hidden own tasks removed;
  same-prefix task with another CLI path and unrelated-name task with the same CLI
  preserved. Config flags set off; unowned user file and SQLite database preserved.
  No portable.flag installed. A future-schema cleanup failure was injected before
  x64/ru uninstall: application files and task records remained; retry succeeded.
  Only UUID-named temporary tasks/QA installations were created or removed.
  Evidence: qa/installer-smoke.json; logs retained under target/inno-smoke-*.
- Real portable GUIs both report 0.1.0-alpha, pass cancellation during a 512 MiB
  transfer and three close/reopen cycles with native notifications. Settings and
  backup directories of the user's existing copies were not reset or migrated.
- Defender native scans of both three-EXE packages and distribution installers,
  plus runtime observation after tests, with protections enabled. See the latest
  qa/defender-verification*.json; no universal antivirus verdict implied.
- Graphify refreshed: 785 nodes / 1707 edges / 52 communities. Existing C# Escape
  extractor self-loop and unavailable SQL parser remain; Inno Pascal is not parsed.

Native 32-bit Windows, clean-machine prerequisite installation, Linux/macOS and
authenticode signing remain unverified/not provided. Inno requires WebView2 to be
installed separately. There is no automatic portable-to-installed state migration.
See INSTALLER.md for F9/ISCC instructions and exact uninstall ownership rules.

# x64/x86 performance update — 2026-10-20

Both portable folders rebuilt from the shared optimized core/frontend. User data
and schedules were not reset. Changes remove repeated ancestor traversal, prepare
scan/resource/revalidation SQL once/reuse bytecode, pause hidden progress polling
and rerenders, and coalesce unchanged tray percentage messages. All original
flush/readback, archive, identity, cancellation and deletion safeguards remain.

Actually run on Windows 10 19045 x64 (x86 via WOW64), local disposable fixtures:
- cargo test --workspace --locked, with and without --target i686-pc-windows-msvc:
  53 passed per architecture, 4 opt-in tests skipped. New tests block a replaced
  parent junction, including when the requested leaf does not exist.
- cargo clippy --workspace --all-targets --locked -- -D warnings, for both targets;
  cargo fmt --all -- --check: passed.
- node --test tests/state-refresh.test.mjs tests/ui-format.test.mjs: 15 passed,
  including hidden/visible polling, focus coalescing, no overlap and retry behavior.
- Tauri production GUI, TypeScript/Vite and release CLI: passed x64 and x86.
- Explicit release performance_10k_small_files test: passed both targets, before
  and after; raw output and measured CPU/RAM in qa/perf-*.txt. Observed initial
  times ~160→128 s (x64), ~190→150 s (x86). Not controlled single-machine A/B:
  runs/other QA overlap. x86 unchanged-preview sample +0.54 s is flagged for
  isolated follow-up; no across-the-board speedup claim. See PERFORMANCE.md.
- scripts/measure-idle.ps1 before/after for both packages: CPU/memory recorded for
  the isolated GUI/WebView process tree. No material idle footprint change claimed.
- test-window-lifecycle.mjs --cancel --cycles=3 --notifications and --stay-hidden
  passed for BOTH optimized packages, real controls, 512 MiB copy, streamed SHA-256,
  safe cancellation/reopen and exit only when completion finds the window hidden.
- Explicit windows_live scheduler test passed for both final packaged CLIs:
  real GUI-closed copy, minutes/hours/days/logon registrations, UUID task removed.
- test-activity-host.py --measure passed for both packaged tray helpers.
- Defender custom scans and post-runtime reports with protection enabled:
  qa/defender-verification.json and qa/defender-verification-x86.json.
- Graphify: 776 nodes, 1691 edges, 48 communities; no missing/dangling endpoints
  or collapsed/duplicate edges; existing C# extractor self-loop and unavailable
  SQL parser documented in architecture/graph/performance-diagnostics.json.

Native 32-bit OS, actual NAS, many concurrent jobs and platform/hardware-specific
performance remain unverified. Timing observations are not universal guarantees.

# Windows x86 portable build — 2026-10-20

Output: portable/NAS Backup Manager x86/ (ordinary folder, no installer/ZIP).
GUI, CLI and notification host are PE machine 0x014c / x86; packaging checks all
three headers before copying. Default x64 artifacts/settings remain separate.
The C# compiler and COM PROPVARIANT layout now respect target architecture.
CLI uses the existing locked static_vcruntime 3.0.0 build helper, matching Tauri;
dumpbin /dependents confirms no VCRUNTIME140.dll import in the final x86 CLI
(qa/x86-cli-imports.txt). No crate versions or npm lock file were updated.

Actual test host: Windows 10 build 19045 x64. x86 processes ran under WOW64.
This does NOT constitute testing on a native 32-bit OS; that gate remains open.
No x86 Windows VM, real NAS or removable-drive hardware was used.

Commands/results:
- rustup target add i686-pc-windows-msvc: official target installed.
- scripts/package-portable.ps1 -Architecture x86: production Tauri 2 GUI,
  TypeScript/Vite and release CLI passed. Subsequent CLI-only linkage fix rebuilt
  release CLI, then -Architecture x86 -SkipBuild refreshed the package.
- cargo test --workspace --target i686-pc-windows-msvc --locked: 51 passed,
  4 opt-in tests skipped. Includes archive/restore/deletion safeguards, cancellation,
  SQLite migrations and streaming; only disposable/in-memory data.
- cargo check and cargo clippy --workspace --all-targets --target
  i686-pc-windows-msvc --locked -- -D warnings: passed (check omits clippy flags).
  cargo fmt --all -- --check: passed. x64 storage regression: 9 passed, 1 skipped.
- With NAS_BACKUP_TEST_PACKAGE set to the absolute x86 portable folder:
  test-window-lifecycle.mjs --cancel --cycles=3 --notifications, --cycles=3
  --notifications, and --stay-hidden all passed. Real GUI, 512 MiB copying with
  streamed SHA-256 comparison, close/reopen, cancellation, GUI preservation and
  completion while hidden verified. Screenshot: qa/sync-cancel-x86.png.
- test-activity-host.py --show-notifications --cancelled and --show-notifications
  --error: passed Russian WinRT notifications and tray lifetime.
- test-activation.ps1: x86 shortcut/COM registration, idempotent setup, protocol
  activation and reusing one window passed; original x64 registration restored.
- NAS_BACKUP_TEST_CLI=<absolute x86 CLI> cargo test -p nas-backup-scheduler
  --target i686-pc-windows-msvc --test windows_live --locked -- --ignored --nocapture:
  passed actual GUI-closed headless copying and minute/hour/day/logon registration;
  UUID test task removed. Repeated after the final CLI runtime-linkage change.
- Defender custom scans and runtime observation with protection enabled: no new
  project detections; qa/defender-verification-x86.json records interval and hashes.
  This is an observation on this host, not a universal antivirus verdict.
- Graphify refreshed: 771 nodes, 1683 edges, 45 communities; no dangling/missing
  endpoints or duplicate/collapsed edges. One existing C# Escape extractor self-loop
  remains; SQL parser unavailable. See architecture/graph/x86-diagnostics.json.

Reproduce with scripts/package-portable.ps1 -Architecture x86. Native 32-bit
Windows 10 verification and clean-machine prerequisites remain release gates.

# Cooperative synchronization cancellation — 2026-10-20

The compact Home progress panel has a real Cancel button for manual and scheduled
workers, including after reopening the GUI. A token-bound local marker requests
cooperative stopping; no process is killed. Committed copies and archives remain,
only the worker's own incomplete staging file is removed. Current archiving/system
I/O may finish before stopping. Schedule settings remain unchanged; cancelled runs
are neutral in the dashboard/history and skip retention. SQLite schema stays 10.

Actually checked on Windows 10, disposable local folders only:
- cargo test --workspace --locked: 51 passed, 4 opt-in tests skipped. New tests
  cover both archive modes, repeated requests, stale tokens, preserving the old
  destination/deletions, keeping committed files and scheduled scan cancellation.
- cargo clippy --workspace --all-targets --locked -- -D warnings and cargo fmt
  --all -- --check: passed.
- Node UI/state tests: 13 passed. TypeScript, Vite, production Tauri GUI and release
  CLI builds passed; the portable folder contains the updated binaries.
- node scripts/test-window-lifecycle.mjs --cancel --cycles=3 --notifications:
  passed actual Preview/consent/Run/Cancel controls during a 512 MiB transfer,
  three close/reopen cycles, neutral cancelled status, no partial destination,
  successful native feedback and an open GUI remaining visible after cancellation.
  Screenshot: qa/sync-cancel.png. An initial test-only IPC interception failed to
  observe the frontend invocation; assertions now read real UI/backend state.
- python scripts/test-activity-host.py --show-notifications --cancelled: passed
  native tray lifecycle and two WinRT notifications with correct Russian text.
- New release EXEs scanned and exercised with Defender protections enabled;
  qa/defender-verification.json records the current observation window and hashes.

Cancellation over a real NAS and Linux/macOS native behavior remain untested.

# Notification packaging repair and local Defender verification — 2026-10-20

The earlier quarantined artifact was not restored. Source changes remove runtime
executable extraction: GUI/CLI now use the explicit nas-backup-notify.exe packaged
beside them, stream its SHA-256 and reject a missing/changed/non-file component.
The build shares a source/compiler-keyed, file-locked native artifact across GUI
and CLI feature graphs; Framework csc is not deterministic across clean builds.
Registration compares current values and the owned shortcut before writing;
unchanged registration preserves the shortcut bytes and modification time.
No Defender exclusions/policy changes, obfuscation or quarantine restore
were used. This is an auditable packaging/setup change, not a Microsoft verdict
that the previous detection was incorrect.

Observed on Windows 10 build 19045, Defender signatures 1.461.102.0, with antivirus,
real-time protection and behavior monitoring enabled:
- Custom Defender scans of all three new release EXEs: no new detections.
- After native notifications, error notifications, protocol activation and real
  test backups, Defender events 1116 and threat records contain zero new project
  detections during the recorded observation window. Artifacts remain readable.
  Evidence: qa/defender-verification.json (UTC interval, hashes, OS and protection
  state; no user data). Reproduce with scripts/test-defender.ps1 -Scan -Since $start,
  the runtime checks below, then scripts/test-defender.ps1 -Since $start, where
  $start=[datetimeoffset]::UtcNow is captured before the initial scan/tests.
- cargo test --workspace --locked: 49 passed, 4 opt-in tests skipped. New sidecar
  test rejects tampering/missing files/directories and verifies no runtime writes.
- cargo clippy --workspace --all-targets --locked -- -D warnings: passed.
- Tauri CLI production frontend/desktop and release CLI builds: passed.
- test-activity-host.py --show-notifications and --show-notifications --error:
  passed, correct Russian names in real WinRT toast history; error icon remains
  temporarily visible and exits automatically.
- test-activation.ps1: passed repeated setup without shortcut rewrites, protocol
  launch and repeated activation reuse one window; identity restored afterwards.
- test-window-lifecycle.mjs --cycles=5 --notifications: passed five close/reopen
  cycles in one 512 MiB copy, SHA-256 matched, no notification failures in audit,
  and the reopened GUI remained visible/alive after completion, until explicitly
  closed by the test. --stay-hidden --notifications also passed: hidden completion
  succeeded and exited without opening the interface.
- Opt-in scheduler windows_live test: passed real headless execution, zero native
  presentation failures, no executables created in state/native, native interval/
  wake/logon registration checks and removal of its disposable UUID task.
- smoke-portable.ps1: passed real packaged GUI startup without Vite/dev server,
  isolated schema 10, integrity_check=ok and zero configured jobs.

All backup tests used disposable local folders. Real NAS files were not touched.
These observations apply to these exact binaries and this verification window;
they do not establish universal antivirus acceptance or a vendor false-positive
decision. Original quarantine history is retained below for traceability.

# Previous artifact: Microsoft Defender quarantine — 2026-10-19

At 23:49:52–23:50:10 Asia/Yekaterinburg, Defender detected
Behavior:Win32/Execution.A!ml (ThreatID 2147735815, critical / suspicious
behavior), terminated the portable desktop process and successfully quarantined
`portable/NAS Backup Manager/NAS Backup Manager.exe` and its Start Menu shortcut.
Operational log events 1116/1117 identify the native activity helper and
RuntimeBroker in the detection chain. This explains the reported disappearance
of the EXE and involuntary GUI closure after synchronization. It is not evidence
that the GUI intentionally closed a visible window on normal completion.

Read-only Get-MpThreatDetection/Get-MpThreat and the Defender operational log were
inspected. Reading the remaining target/release desktop artifact for SHA-256 and
Authenticode was blocked as containing a virus or potentially unwanted program;
a Defender custom scan also recorded that build artifact at 23:53:49. No passing
antivirus result or confirmed false-positive verdict exists. Previous functional
test passes do not override this release blocker.

No protection settings were changed, no exclusions were added and no quarantined
artifact was restored or recopied into the portable folder. Source review must
investigate notification/shortcut/protocol registration and helper activation;
release requires resolving the detection, not simply renaming/repacking the EXE.
The detection cause is not conclusively identified by the event chain alone.

# Repeated window close/restore regression — 2026-10-19

Reproduced the follow-up failure against the preceding release: the second close
after native restoration left the window visible. Tao caches a VISIBLE flag for
set_visible even though is_visible queries Windows directly. ShowWindowAsync had
restored the actual HWND without changing that cache, so another hide was a no-op.
The active-work close callback now reconciles the toolkit visibility first, then
hides in the same event callback. Backup execution and dependencies are unchanged.

Executed on Windows:
- `node scripts/test-window-lifecycle.mjs --cycles=3` against the old build failed
  on the second hide, reproducing the report.
- `node scripts/test-window-lifecycle.mjs --cycles=5` against the fixed release
  passed five close/reopen cycles during one 512 MiB local transfer using the same
  GUI process. Copy succeeded, streamed SHA-256 matched and the idle close exited.
  The script now defaults to five cycles instead of one.
- `cargo test -p nas-backup-desktop --locked`: 2 passed, 1 OS-scheduler test skipped.
- `cargo clippy -p nas-backup-desktop --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- Tauri CLI production build (including TypeScript/Vite) passed; portable EXE
  updated while preserving data/. Tests touched no user backup folders or NAS.

# Window close during manual synchronization — 2026-10-19

Root cause: manual execution lives in the desktop blocking worker. Closing its
last window previously terminated that process and left a genuinely interrupted
run for recovery. The desktop now retains/hides its window while filesystem work
is active, including final notifications and first schedule activation. Once all
work ends, a hidden desktop exits; a reopened visible desktop stays open. No idle
service, new dependency, database migration or change to copy safety was added.
Historical interruptions receive their own RU/EN explanation rather than the
fallback filesystem-access message.

Repeated EXE launch and tray activation find the exact application window even
when hidden. Process.MainWindowHandle was insufficient: Windows could return an
untitled auxiliary HWND. Activation now checks process executable path and window
title before restoring it.

Executed on Windows, using isolated copies of the actual release GUI:
- `node scripts/test-window-lifecycle.mjs`: passed. A 512 MiB disposable local
  file was copied while closing/reopening the native window; the same original
  process/webview was restored, the run succeeded, streamed SHA-256 comparison
  matched, and closing the idle GUI exited.
- `node scripts/test-window-lifecycle.mjs --stay-hidden`: passed. Closing during
  transfer left copying active, the destination matched, SQLite recorded success
  and the hidden process exited automatically after completion.
- These scripts enable loopback WebView CDP only in their child process, disable
  notifications only in their disposable database, use no NAS/user files and
  remove their two large fixture files after success. No production debug port
  was added. Failed fixture directories remain under target/ for diagnosis.
- `cargo test --workspace --locked`: 48 passed, 4 opt-in tests skipped;
  focused desktop tests rerun after the final native activation correction.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `npm run build` (TypeScript + production Vite): passed outside the sandbox;
  the sandbox had rejected esbuild child spawning with EPERM.
- Tauri 2 release and CLI built; portable folder updated preserving data/.

An attempted native WebView test inside the Rust test harness failed with
WebView2 initialization/crash errors and displayed a Windows error dialog. That
harness was discarded; the two passing tests above exercise the regular release
application instead. Linux/macOS lifecycle and actual NAS transfers were not
validated by these local Windows fixtures.

# Activation, stable refresh, verified progress and ETA — 2026-10-19

Implemented: single left tray click opens/focuses the GUI; new ToastGeneric
notifications protocol-activate the GUI after the worker has exited. An owned
per-user URL registration uses a fixed --show command; arbitrary URI input cannot
supply filesystem paths or shell commands. A path-checked focus helper reuses the
same application window without a resident listener or added dependency.

scripts/test-activation.ps1 passed on Windows with a UUID-named portable clone:
protocol launch opened the GUI, the shared tray action and repeated protocol
activation reused that same PID/window. Only clone processes were stopped; owned
protocol/shortcut registration was restored to the actual portable application.
This tests activation routing and the shared handler, not a physical pointer click
in Notification Center. Native toast smoke accepted both new protocol notifications
with correct Unicode job names and entries in the app notification history.

Fixed layout jumps: unchanged job lists retain identity on progress-only database
commits, summary data is retained during refresh, and progress labels occupy stable
slots. Capacity measurement is allowed while the selected worker is active, including
on reopen; it remains event-driven, not a NAS polling loop. Browser mockIPC QA showed
ETA and storage values during an active run and after navigating away/back. Evidence:
qa/progress-eta-storage.png. Real NAS/3.5GB user files were not accessed by QA.

Migration 10 persists readback bytes + ETA locally. Copying, flushing and verifying
are separately reported; SHA-256 readback is preserved. Overall active progress
combines copied/readback bytes and stays below 100 until successful completion.
ETA uses a bounded recent-throughput window, supports batches of small files and
discards stalled samples. Unknown phases/timings explicitly show calculating.

Checks performed: final workspace Rust suite 47 passed / 4 opt-in skipped;
strict workspace Clippy passed. TypeScript/frontend production build and Tauri 2
release build passed. Node formatter/state tests: 13 passed. New tests cover both
ETA modes, readback progress persisted across connections, flushing with unknown
ETA, and capacity queries during an active disposable copy. Native GUI-closed
scheduler integration separately passed with the new schema/CLI. Portable smoke
launch without a dev server returned schema 10 / integrity ok / zero jobs in a
UUID-named clone. Real user settings/backup data were preserved. Linux/macOS
activation and physical Notification Center pointer clicks were not validated.

---

# Startup scheduling and compact home progress — 2026-10-19

Added "При включении ПК" / "When the computer starts" to the existing schedule
selector. Windows uses a current-user LogonTrigger with a 30-second delay;
no GUI, service, elevated account or Windows login password is required. It runs
on sign-in after boot (and after a later sign-out/sign-in), not before login,
on unlock, or merely on resuming from sleep. Existing consent and safety gates
remain. Interval/time/wake controls are hidden for this event mode.

Migration 9 preserves old schedules and exposes startup through a separate
schedule_on_login flag, avoiding a jobs table rebuild. Tests cover new startup
jobs, update/reload, returning to intervals and wake=false normalization. Display
uses an event-based next-run description instead of a false calendar timestamp.

Checks: workspace Rust tests 45 passed / 4 opt-in skipped; strict Clippy passed.
Focused startup test rerun after adding creation coverage: passed. TypeScript /
production frontend build passed. Node formatting/refresh tests: 10 passed.
The opt-in Windows integration test separately passed: real disposable scheduler
registration retained its user-specific LogonTrigger and PT30S delay, headless
CLI copying still worked, and the test registrations were removed. Reboot/sign-in
was not forced or exercised. Linux/macOS startup adapters were added but not
runtime-tested on this Windows host.

Progress now appears only on Home, in a compact block with a thin bar and a
single details row. A manual run closes Preview and opens Home. Browser mockIPC
QA verified Home progress and absence in Settings while the same background run
remained active. Screenshot: qa/compact-home-progress.png (explicit QA fixture).
Tauri 2 release build and updated portable folder passed smoke launch with no dev
server: schema 9, integrity ok, zero jobs in a disposable clone. User settings
and backup data were preserved.

---

# Progress, Windows tray and notifications — 2026-10-19

Implemented and checked on this Windows machine:
- Migration 8 adds default-on notifications_enabled and local job_activity records.
  Setting survives reopen; no NAS credentials or backup data are migrated.
- Manual and scheduled execution publish real phases/counts/bytes/current path.
  Local file locks suppress stale progress after a crash. UI initial load reads
  headless activity; active polling is local/visible-only, with no idle NAS scans.
- Stale previews and files changed during copying show a neutral waiting message,
  retain safe execution refusal, and are excluded from error-only run filters.
  Real failures still count as errors. Job rows are compact.
- Windows start/completion/error toasts use explicit UTF-8 streams. Native smoke
  verified correct Russian job text, successful WinRT calls and entries in the
  application notification history. Muted success/error tests sent zero toasts.
  Error status keeps the red tray icon for 30 seconds, then the host exits.
- UI setting is enabled by default and persists for GUI/headless execution.
  Turning it off preserves progress and tray status.

Validation actually run:
- cargo check --workspace --locked; cargo fmt --all -- --check; strict workspace
  Clippy with all targets: passed.
- cargo test --workspace --locked: 44 passed, 4 opt-in skipped. New cases exercise
  file-backed cross-connection activity, exclusive locks, stale-worker cleanup,
  copied-byte totals, neutral stale-preview status and genuine error counts.
- Windows native scheduler integration separately passed with a UUID-named task
  and disposable folders. GUI-closed copying, direct CLI completion, live progress
  cleanup and zero native-presentation failures were checked. Test task removed.
- Node formatting/local observer tests: 9 passed. Python SQL constraints: 7 passed.
- TypeScript check, production frontend build and Tauri 2 release build passed.
  Release CLI built with --locked; desktop/CLI portable EXEs updated without an
  installer. Smoke launch without a dev server reported schema 8/integrity ok and
  zero jobs in an isolated cloned folder; actual user data was not reset.
- Browser QA used explicit mockIPC fixtures only: progress on opening during a
  background run, automatic removal on completion, compact job rows, neutral
  source-change message, notifications switch initially on and toggled off.
  Evidence: qa/sync-progress.png, qa/source-changed.png,
  qa/notification-settings.png. This is visual QA, not fabricated backup history.
- python scripts/test-activity-host.py --error --show-notifications --measure:
  passed. Two real test toasts accepted; Russian text round-tripped; error icon
  stayed active and host exited automatically. Working set 44,027,904 bytes,
  peak working set 44,556,288 bytes; CPU-time delta 0 ms over a 2.281 s quiet
  active-host sample. This is the transient tray host only, not a whole-app idle
  or throughput benchmark. No permanent service is installed.

Additional disposable local benchmark:
`cargo test -p nas-backup-core --test backup performance_10k_small_files --locked -- --ignored --nocapture`
passed (158.24 s total, debug build, in-memory metadata DB, 10,000 files of 20 bytes).
Fixture creation 2.443 s; initial preview + approved execution 146.923 s;
unchanged preview 6.753 s. A working-set sample during the test was 18,685,952
bytes (not a measured peak). No NAS/network throughput or release performance is
inferred. Log: target/activity-10k.log. CPU/RAM model metadata was not collected:
the optional read was rejected when automatic approval review was unavailable
at the service usage limit; no bypass was attempted.

Graphify regenerated 679 nodes / 1490 edges / 40 communities. SQL AST support is
not installed; one C# Escape name-resolution self-loop remains a tooling artifact.
Final graph check found zero missing/dangling endpoints; no duplicate edges were
reported by Graphify diagnostics.

Limitations: native presentation currently Windows x64 only; Linux/macOS are not
validated for these changes. Windows focus/notification settings may suppress
visible messages. No live NAS, real user backup tree or credentials were changed.
Full RAM/CPU/throughput qualification across all required release scenarios is
still outstanding; the small host sample above is not a release-wide budget.

---

# Manual mode and approved schedule activation — 2026-10-19

Migration 7 introduces schedule_automatic / schedule_pending while keeping actual
registration in schedule_enabled. Existing enabled schedules stay automatic;
previously disabled jobs stay inactive/manual and are never enrolled on upgrade.
Stored intervals are preserved. SQL triggers prevent enabling manual jobs.
New jobs default to manual mode. Manual jobs do not schedule or wake the PC.

Selecting recurrence stages activation after successful execution of a consented
preview. The preview checkbox explicitly authorizes the selected recurrence and
its archive/deletion policy. Refusal/failed copying cannot activate scheduling.
Registration errors are returned separately from backup success. Manual pause
clears pending activation, so ordinary manual runs do not undo it.

Checks: cargo check, strict Clippy, production frontend/TypeScript build passed.
Workspace Rust tests: 42 passed / 4 opt-in skipped. New tests cover migration from
v6 with active and disabled jobs, manual scheduled-run refusal, approval gates,
and pending-state transitions. The new opt-in desktop activation test separately
passed with a disposable Windows task and files: no enrollment before consent,
registration after successful copy, visible next run, missing CLI failure without
false activation, manual refusal and disable/cancellation. Test registration was
removed. tempfile was already in Cargo.lock; adding it to desktop dev-dependencies
adds no shipping dependency. Eight Node formatting/refresh tests passed.

Browser QA uses explicit mockIPC only: manual option hides interval/wake controls,
manual switch=false/disabled; recurrence preview displays automatic-activation
consent and switches on after approved mocked completion. Screenshots
 design/manual-mode-form-qa.png, design/manual-schedule-qa.png and
 design/automatic-schedule-qa.png show test fixtures, not actual backup results.
Real backup paths, user jobs, credentials and schedules were not changed by QA.
Graphify refreshed; seven SQL migrations lack optional SQL AST extraction.
Linux/macOS runtime validation remains outstanding.

Production Tauri/CLI builds and isolated portable smoke passed with Vite stopped:
schema=7, integrity_check=ok, zero jobs. Release CLI still has PE subsystem=2
(no console allocation). The separate Windows headless scheduler integration
passed using this release CLI and removed its disposable task. No installer or
ZIP was created. Real app state was not migrated/reset by the tests.

# Silent Windows headless execution — 2026-10-19

The scheduled nas-backup.exe now uses the Windows GUI subsystem on Windows.
No helper/process-launch behavior or backup operations changed: network and
scheduler helpers already set CREATE_NO_WINDOW. No new dependencies.

Built cargo build --release --locked -p nas-backup-cli. Inspected the release PE
header: subsystem=2 (Windows GUI), so Windows does not allocate a console for the
scheduled process. Redirected --version output and invalid-command stderr/nonzero
exit code passed. The Windows live scheduler test passed using this release EXE:
disposable-file backup ran headlessly, settings/next triggers were verified and
the disposable OS registration removed. No hardware sleep test or visual window
recording was performed. Only the CLI sidecar and documentation in the portable
folder were updated; real settings, schedules, backup files and archives untouched.

# Shared state refresh, wake option and job menu — 2026-10-19

SQLite migration 6 adds wake_computer=false to new/existing jobs. JobDraft/SavedJob
and RU/EN forms carry the setting; Windows XML sets WakeToRun explicitly and
Hidden=false. AppStatus reports whether native wake is supported. Unsupported
platform wake requests fail visibly without writing scheduler files.

Added a persistent local SQLite data_version observer and a visible-interface
5-second check. Actual changes refresh jobs and dashboard; stale list responses
are ignored, requests serialized, hidden checks paused, transient refresh errors
retried. Capacity does not follow unchanged metadata refreshes. There is no NAS
polling or filesystem scan for change detection. No new sidebar/card was added,
per the user's clarification.

The row menu exposes Edit, Pause and Remove on home/jobs. Remove confirmation
states copies/archives remain; calls schedule-disable before soft-removal. Existing
temporary-directory tests verify removed-job archives remain recoverable with
source offline. User jobs/settings/schedules/files were not modified during QA.
Read-only inspection found the current gh job schedule disabled; no matching OS
registration was present at inspection time. No automatic repair was performed.

Checks: workspace Rust tests 40 passed / 3 opt-in skipped; the opt-in Windows
scheduler integration separately passed with disposable task/files, confirmed
headless copy, WakeToRun true/false, Hidden=false, actual future triggers and
removal. Hardware sleep/wake was not exercised. Strict Clippy and TypeScript
passed. Seven frontend Node tests passed, including local-refresh coalescing,
visibility pause and retry-after-error.

Browser QA used explicit mockIPC fixtures only: independently changed the job's
lastRun, waited for observer refresh and confirmed both displayed timestamps
advanced to the same value without manual refresh. Menu confirmation and call
order schedule -> remove -> listJobs passed. Wake is initially off and appears
in review when selected. Screenshots design/job-actions-qa.png and
 design/wake-review-qa.png are labeled QA fixtures, not real backup results.
Graphify regenerated using architecture/README.md commands; six SQL migrations
lack optional tree_sitter_sql extraction and are covered by SQLite tests instead.
Linux/macOS runtime verification remains outstanding.

Final Windows production frontend/Tauri/CLI builds passed. The ordinary portable
folder was refreshed; no installer/ZIP was created. Isolated native smoke without
Vite passed: schema=6, integrity_check=ok, zero fixture jobs, application remained
alive. Only the smoke process was stopped; real state was not migrated/reset by QA.

# Count-based protection, capacity refresh and project links — 2026-10-19

Migration 5 preserves existing jobs and adds guard enabled=true / minimum=10.
No percentage test remains. Temporary-directory scheduled tests cover deletion
counts below/at threshold, differing totals, disabled protection, archived and
no-archive policies, logged refusal before mutation, stale approval after edits,
and source-offline refusal with protection disabled. No real user files or NAS
were used. Workspace Rust tests: 39 passed, 3 opt-in skipped; extended count cases
retested separately. cargo check and strict Clippy passed. Seven initial-schema
Python tests and four frontend formatting tests passed.

TypeScript and production frontend build passed. Browser component QA used the
real JobForm and Dashboard with Tauri's mockIPC in the explicit test-only page.
Verified guard toggle, retained custom count, warning and inputs; capacity requests
on mount, job change, post-sync jobs refresh and manual refresh, with visible
selection-specific results. Mock data are excluded from production. Screenshot
`design/deletion-guard-qa.png` shows a component fixture, not a real backup run.
Verified About links match both user-provided GitHub URLs. Native OS browser
opening and real NAS capacity were not exercised by browser QA.

Capacity requests are serialized and stale responses ignored; no interval polling
was added. Graphify regenerated; five SQL migrations lack optional SQL AST parsing,
so schema behavior is covered by SQLite migration tests instead.

Windows release packaging completed with Tauri --ci --no-bundle and the release
headless CLI. scripts/smoke-portable.ps1 launched a separate copy with no Vite
server: schema=5, integrity_check=ok, zero jobs, process remained alive. Only the
test process was stopped. The portable folder was updated without resetting data.
Linux/macOS were not runtime-tested during this change.

# Explicit guest mode — 2026-10-19

Added a Windows/guest/credentials connection selector. Guest mode saves without a
username/password form, restores from the per-share vault profile and is used by
the common headless/GUI reconnect adapter. Preference saving does not contact NAS
or claim authentication succeeded. Empty passwords remain explicit String.Empty;
no Windows SMB policy, signing or encryption setting is modified.

Checks: workspace Rust tests 37 passed (3 opt-in skipped); strict Clippy passed.
The opt-in Windows vault round trip passed for credentials -> tagged guest with
zero-byte password -> removal/automatic Windows mode. Test targets were unique
and disposable; no real credentials/NAS used. Initial helper compilation exposed
a C# local-name scope collision; renamed the variable and repeated native test
successfully. UI QA confirmed guest Save enabled without fields, credentials Save
disabled without a password; production typecheck/frontend/Tauri builds passed.
Screenshot docs/design/guest-access-qa.png is a labeled component fixture, not a
successful real-NAS connection. Reboot/NAS authentication remains unvalidated here.

Graphify updated using the commands in architecture/README.md; SQL and embedded
C# extraction limitations remain. Existing jobs, backup files and archives are
unchanged. No settings reset or user-data operations performed.

# NAS connection, direct destination and UI corrections — 2026-10-19

Delivered changes:
- New jobs put active files directly in the selected empty destination. An isolated
  .nas-backup-manager service tree holds archive/staging/ownership/lock metadata.
  Source entries using that root name are rejected; mirroring excludes the service
  tree. Existing jobs default to the old layout and are not moved, per user request.
- Windows NAS access: per-share Generic Credential Manager records, WNet reconnect
  before explicit/headless operations, 30-second helper timeout, no per-file helper,
  no retries in idle. Current available Windows sessions are reused. Credential
  conflict 1219 is visible; other sessions are never forcibly disconnected.
- Secrets use a private stdin pipe and managed JSON deserialization, never process
  arguments, SQLite, temporary files or application logs. Only numeric/sanitized
  errors leave the helper; UI does not read passwords back from the vault.
- Shared success/info/error/clock badges across job rows, latest sync and journals;
  RU/EN plural forms, normalized display-only Windows prefixes, and local-time
  greeting refreshed at hour boundaries/visibility changes.

Actual checks on this Windows 10 machine:
- cargo test --workspace --locked: 36 passed; 3 explicit opt-in tests skipped.
- Node formatter tests: 4 passed (Russian singular/few/11–14/21/22, EN forms,
  file/day counts, display prefixes and greeting boundaries). Command:
  node --experimental-strip-types --test tests/ui-format.test.mjs
  Sandbox initially blocked the Node child process (EPERM); permitted retry passed.
- Python initial-schema tests: 7 passed. Rust migration tests upgrade persisted
  settings/jobs to schema 4 and verify legacy layout defaults.
- cargo check, fmt and strict workspace/all-targets Clippy passed.
- Windows Credential Manager opt-in round trip passed using only a unique dummy
  credential, then verified deletion. No real NAS/password was read or used.
- Windows Task Scheduler opt-in test passed: headless local fixture copied with
  GUI absent, next actual run queried, minute/hour/day registrations accepted,
  disposable task removed. This did not test authentication to a real NAS.
- New core tests cover direct paths, reserved service-tree rejection, unrelated
  nonempty destination protection and continuing legacy archive/restore behavior.
- UI QA fixture visually checked filled green checks, blue archive information,
  neutral unchanged state, errors/running, greeting, NAS form and wizard review
  showing “Каждые 2 минуты”. Fixture is explicitly labeled and excluded from the
  production frontend. Screenshot: docs/design/status-icons-qa.png.
- TypeScript/frontend build and Tauri 2 no-bundle release/CLI build passed.
  Isolated packaged startup passed with Vite stopped: schema 4, integrity ok,
  zero jobs, metrics/events tables present. Closed only the created process.
- Graphify: 514 nodes, 1147 edges, 30 communities. Diagnostics: no missing/dangling
  endpoints, self-loops or duplicate edges. Optional SQL parser absent (4 migrations
  omitted); embedded C# in PowerShell is not represented as an AST call graph.

Resource-cost review: base64 0.22.1 already existed in Cargo.lock; now used by the
OS adapter. Removed storage's unused core dependency, added core -> storage and
serde_json for the typed pipe payload. No installer, daemon, telemetry, custom SMB
stack or extra runtime installation. Built-in PowerShell/.NET is launched only for
network actions (fast accessible-session path skips C# compilation); this adds
active-operation startup cost, not an idle worker. Performance benchmarks were not
rerun; no new throughput or total idle-resource budget is claimed.

Remaining: reboot-to-real-NAS authentication, wrong/removable mounts, native picker/
Windows scaling, full fault/performance campaigns and Linux/macOS runtime checks.
Windows user must be signed in for the existing InteractiveToken scheduler. This
update does not configure backups before OS sign-in. Credential UI is Windows-only;
other OSes continue to use OS-mounted shares. No existing user backup/archives were
migrated, moved or deleted by development or tests. Portable data/ is preserved.

Historical records below describe earlier builds.

# Flexible schedules, retention policy and reference dashboard — 2026-10-19

Compared repository design-reference.png with the current UI. Added the reference's
job/storage split, latest synchronization counts/bytes/duration, archive count/size/
expiry, schedule switch, recent per-file outcomes and functional Help/About dialogs.
Capacity is measured on demand; no invented archive quota or scan-time measurement.
Browser shell screenshot: docs/design/current-interface.png. Browser checks covered
new sections, compact accessible pagination, Help and journal-view switching. This
is not native populated-screen, folder-dialog or Windows scaling sign-off.

Migration 0003 preserves existing daily/retained-archive jobs; adds minute/hour/N-day
schedules, explicit no-archive policy, run metrics and bounded per-file journal.
The user's explicit no-archive request supersedes the original always-archive rule
for opted-in jobs. Revision-bound preview consent explicitly describes permanent
removal; default remains 30-day archive. Existing archive payloads/dates are preserved.
Confirmed restore-over-existing still archives the collided file as explained in UI.

Actual Windows checks:
- cargo test --workspace --locked: 32 passed, 2 opt-in tests skipped.
- cargo check, cargo fmt --all --check and Clippy --all-targets -D warnings passed.
- Python schema tests: 7 passed; migration upgrade/defaults also tested in Rust.
- TypeScript and production frontend build passed; Tauri 2 no-bundle release and
  headless CLI built successfully. npm/Cargo lockfiles retained.
- Opt-in live Windows test passed: GUI-closed headless copying, actual next-run
  query, registration of 15-minute, 2-hour and 3-day schedules, removal of temporary
  task verified. Only disposable data/unique task name were used.
- Isolated packaged startup with Vite stopped passed: schema 3, integrity ok, zero
  jobs, metrics/event tables present. Only the created test process was closed.
  Reusable script: scripts/smoke-portable.ps1. The first ad-hoc validation failed
  on PowerShell/Python quote escaping; the script fixed it and passed.
- New deterministic tests: opt-in permanent replace/delete, refusal without consent,
  old archive preservation/restore, stale preview/wrong owner, locked-source staging
  failure retaining the only backup, calendar cycles across months/missed runs,
  committed dashboard counts surviving preview and later failures, file-history bounds/pagination.
- Graphify refreshed: 480 nodes, 1018 edges, 27 communities; no missing/dangling
  endpoints, duplicate edges or self-loops. Optional SQL parser unavailable; all
  three SQL migrations are absent from the graph and covered by behavior tests.

No permanent worker, NAS polling or transfer-rate limits added. New dashboard reads
only SQLite metadata; capacity reads occur on explicit refresh. Active performance
benchmarks were not rerun for this update. Previous measurements below are historical.
Linux/macOS adapters and OS-specific sleep/DST semantics remain unvalidated here.
Real NAS/removable-drive tests and the full fault/scaling/performance gates below
remain open; this update does not assert a fully validated cross-platform release.

# Functional backup and ordinary-folder follow-up — 2026-10-19

Implemented: job configuration, guarded ownership enrollment, streamed SQLite scan
spool and deterministic plans, preview-bound explicit execution consent, staged
copy/verification, archive-before-replace/remove, collision-bound restore, safe
expiry, soft job deletion preserving archive access, process locks, interrupted
run reconciliation, actual capacity, paged UI/history, RU/EN, native folder plugin,
headless CLI and OS registration. UI development banner/placeholder labels removed.

Actual Windows 10 checks:
- Final EXE also passed an isolated startup check after Vite was stopped: the
  embedded frontend launched, actual desktop IPC created local state, and the
  created test process was closed. No Node/dev server is needed for folder launch.
- Isolated no-install smoke: copied the EXE and portable flag into a fresh target/
  folder, launched it without installation, verified it stayed alive and created
  its own empty schema-2 SQLite state through actual desktop IPC. Integrity was ok,
  default language ru and job count zero. Closed only the created test process.
  Parent process working set was 32,833,536 bytes; parent CPU delta over a 5-second
  idle sample was 0 ms. WebView child processes/GPU were not measured, so these
  figures are not total application RAM/CPU or a passed idle-resource gate.
- `cargo test --workspace --locked`: 24 passed, 2 explicit opt-in tests skipped.
- `cargo check --workspace --locked`, `cargo fmt --all --check`, strict Clippy passed.
- TypeScript and frontend production build passed; Tauri release EXE and CLI built.
- Python in-memory schema tests: 7 passed.
- Opt-in Windows test `headless_task_runs_with_no_gui`: passed. Created a unique
  temporary task/database/source/destination, queried actual next-run time from
  Task Scheduler, ran the headless task, verified the added file, removed the task
  and verified it no longer existed. No GUI or real user backups involved.
- First live scheduling test failed: Task Scheduler rejected the XML encoding.
  Fixed UTF-16 LE + BOM; repeated real task test passed. An initial automatic
  approval timeout was resolved by the permitted retry; no approval blocker remains.
- Core tests cover new/changed/unchanged, Unicode/long paths and empty folders,
  repeated deletion versions, stale preview, explicit consent, offline source,
  archive obstruction, wrong destination identity/owner, locked source, collision
  confirmation and intervening target edits, protected expiry, removed-job restore,
  pending staging recovery and streamed 8 MiB transfer/progress.
- Browser smoke: all navigation exposed, lazy archive/history loaded, RU/EN
  switched, old development wording absent. `current-interface.png` captures the
  browser-only view; no native picker/scaling visual sign-off is implied.

Performance observations (debug test, local temporary NTFS folders, in-memory
SQLite fixture; not a release/NAS throughput claim): Intel CC150 3.50 GHz,
17,088,712,704 bytes RAM, fixed NTFS C: volume. 10,000 files × 20 bytes.
First pass: fixture 2.79 s, initial run 208.09 s, unchanged preview 6.45 s.
After eliminating duplicate file flushes/batching new-file status updates:
fixture 2.37 s, initial 162.49 s, preview 6.58 s. Indexed pending-state update pass:
fixture 2.40 s, initial 164.31 s, preview 6.27 s. Runs had other development activity;
these are observations, not a controlled speedup claim or passed performance gate.
Peak RAM/CPU and network throughput were not measured in those active runs.

Graphify code/Cargo graph updated: 451 nodes, 920 edges, 26 communities. SQL parsing
is unavailable (two migrations omitted from graph); standalone behavior tests run.
No invented semantic graph for docs/images. No background polling or speed caps.

Remaining release gates: real NAS/removable mount swaps/disconnects, path-swap
adversarial races, complete power-loss/disk-full/fault campaigns, many runnable jobs,
full idle/active resource budget, DST/logout tests, native UI/picker/scaling and
Linux/macOS. macOS network enrollment currently fails closed because its safe mount
identity adapter is not implemented. Linux adapters have not been runtime tested.
No claim of a fully validated cross-platform production release is made.

Delivery is `portable/NAS Backup Manager/`: EXE + headless CLI + portable.flag,
application state under data/. No ZIP, installer execution, automatic startup
registration or real-data mutations were performed for delivery.

Earlier-stage records follow for context; their old availability limitations do
not describe the current build.

# Validation record — 2026-10-19

## Toolchain follow-up and desktop build

Actual environment: Windows 10 Pro, OS version 10.0.19045.0, x64 Rust target.
Detected Node v24.22.0, npm 11.19.1, cargo 1.99.1 and rustc 1.99.1.
The original scaffold contained no package.json or npm lockfile. Added the desktop
frontend with npm, retaining the root Cargo.lock generated by the first Rust check
and extending it for required Tauri/SQLite dependencies. Generated package-lock.json
is the frontend source of resolution; no switch to pnpm/yarn.

Commands actually executed successfully:
- `npm.cmd install --no-fund --offline=false` (after sandbox offline/cache errors).
- `cargo --config net.offline=false fetch` (after sandbox offline error).
- `npm.cmd run typecheck` and `npm.cmd run build`.
- `cargo check --workspace --locked`.
- `cargo test --workspace --locked`: **8 passed**, four domain and four storage tests.
- `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `python -m unittest discover -s tests -p 'test_*.py' -v`: **7 passed**.
- `npm.cmd run tauri -- build --ci`: release EXE and Windows x64 NSIS installer.
  NSIS tools were downloaded by Tauri from official tauri-apps release URLs and
  their hashes validated by the bundler. Installer was not installed or published.

Build fixes: applied rustfmt, repaired the shadcn-generated utils import, installed
its missing CVA dependency, removed unused cn dependency. esbuild's child-process
launch was denied in sandbox (EPERM); approved external build succeeded. npm
reported zero known vulnerabilities at install time, which is not a security audit.
The declared Rust minimum changed from 1.85 to 1.90 because cargo metadata reports
Tauri packages requiring 1.90; only installed Rust 1.99.1 was actually tested.

Browser smoke check (local Vite shell): dark initial page, Settings navigation,
RU → EN → RU switching and unavailable job state verified through rendered UI.
Screenshot: `docs/design/development-shell.png`. This does not test desktop IPC,
SQLite persistence through WebView, native folder dialogs, installer execution,
1024×700/scaling, keyboard/reduced-motion acceptance or backup operations.

Graph updated: 244 nodes, 305 edges, 18 communities. Graphify diagnostics: zero
missing/dangling endpoints, self-loops or collapsed undirected endpoint pairs.
SQL parser remains unavailable. Incremental prune warned about four now-excluded
generated Tauri schema files; checked the graph: no nodes reference `/gen/`.
Graph extraction fell back to sequential execution when sandbox denied workers.

No backup source/destination data was touched. No backups, restore, expiry,
scheduling, mount identification or performance results are claimed. Linux/macOS
remain untested. The records below describe the earlier architecture-only pass.

## Initial architecture pass

Environment: Windows PowerShell. Windows version, hardware performance, NAS,
removable disks and other OSes have not been validated.

Discovery:
- Entire repository AGENTS.md read; provided reference image visually inspected.
- UI UX Pro Max and Graphify skill files read from the installed skill directory.
- shadcn MCP discovery succeeded; button/dialog registry query succeeded.
- Graphify Python import and CLI help succeeded.
- `Get-Command cargo,rustc,node,npm,python,git` found Node, Python and Git;
  Cargo/rustc/npm were not found. Standard `~/.cargo/bin/cargo.exe` absent.
- Node's bundled bin directory contains node.exe only. MSVC/WebView build
  prerequisites have not been verified. No installers were run.

Actual verification results are recorded below after executing checks. No Rust,
Tauri, frontend, filesystem executor, scheduling or performance success is implied
by passing SQL tests. No tests touch real backup data.

Executed:
- `python -m unittest discover -s tests -p 'test_*.py' -v`: **7 passed**.
  Covers disabled defaults/preview revision, time and retention constraints,
  active-run exclusivity, independent jobs, incomplete scan success rejection,
  multiple archive versions, foreign keys, protected history/job deletion,
  cross-job archive/run mismatch and database integrity.
  Initial test discovery exposed a helper named `run` overriding unittest;
  renamed it `add_run`, then reran successfully.
- Graphify `extract . --code-only --cargo --out docs/architecture/graph`:
  **96 nodes, 122 edges, 11 communities** after the final code changes.
- Graphify `cluster-only docs/architecture/graph --no-label`: succeeded;
  produced GRAPH_REPORT.md and graph.html with numeric community labels.
- Graphify `diagnose multigraph --graph .../graph.json --json`: zero missing
  endpoints, dangling endpoints, self-loops or collapsed endpoint pairs.
  Graph covers Rust/Python/Cargo structure; SQL extraction warned that optional
  `tree_sitter_sql` is absent. Docs/images deliberately excluded from code-only run.
- Original and copied reference SHA-256 compared; hashes match.

Blocked/not run: Rust compilation/unit tests/format/clippy (Cargo/rustc missing),
Tauri/frontend build (unimplemented; npm missing from PATH), live disk/mount safety,
Windows 10/11 platform acceptance, Linux/macOS, UI visual acceptance, performance.
The SQL tests validate schema constraints, not a production migration runner.
