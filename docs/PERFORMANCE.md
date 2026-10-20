# Performance observations — 2026-10-20

Host: Windows 10 19045 x64, Intel CC150 3.50 GHz, 16 logical cores, 15.92 GiB RAM.
x86 runs under WOW64, not a native 32-bit OS. Tests use local disposable TEMP /
workspace directories; storage medium was not identified. No NAS/network throughput
was measured. Defender protection remained enabled.

## Changes

- Path validation checks each ancestor once per `safe_join` call rather than
  recursively rechecking every ancestor's ancestors. There is no cross-operation
  metadata cache. Junctions/reparse points, traversal, missing/error paths and
  expected resource/ownership checks still apply. Two focused safety tests cover
  missing nested leaves and replacing a previously validated parent with a junction.
- Scan inserts/directory queue operations reuse prepared SQL statements. Resource
  and per-entry revalidation cache SQL bytecode only, never filesystem/identity
  results. Every call still reads current values. SQLite's cache remains bounded.
- Hidden GUI ignores native progress rerenders and stops progress polling timers.
  Reopening immediately requests fresh activity, including during a manual run.
  Focus events coalesce; requests do not overlap; failures remain visible/retryable.
- Rust sends tray progress only when the displayed integer percentage changes.
  Final success/cancellation/error messages and notifications are still sent.

Copy flush/readback verification, archive-before-replace/delete, cancellation,
mount identity and ownership safeguards are preserved. No throughput cap, resident
service, whole-file RAM buffering, working-set trimming or antivirus changes.

## 10,000 small files

Reproduce:

```powershell
cargo test --release -p nas-backup-core --test backup --locked performance_10k_small_files -- --ignored --nocapture
cargo test --release --target i686-pc-windows-msvc -p nas-backup-core --test backup --locked performance_10k_small_files -- --ignored --nocapture
```

Each fixture contains 10,000 files of 19 bytes (190,000 payload bytes). Initial
time includes preview, rescanning and verified copying; unchanged time is a later
dry run. This is dominated by small-file overhead, not a sequential bandwidth test.
Runs of the two architectures overlapped, and builds/QA also ran on this host.
These are single observations, not an isolated statistical speed comparison.

| Observation | x64 before | x64 after | x86 before | x86 after |
|---|---:|---:|---:|---:|
| Fixture creation, s | 2.624 | 2.696 | 2.905 | 2.922 |
| Initial backup, s | 159.906 | 127.591 | 189.777 | 149.895 |
| Unchanged preview, s | 7.218 | 6.730 | 6.673 | 7.216 |
| Initial files/s | 62.5 | 78.4 | 52.7 | 66.7 |
| Engine peak working set, MiB | unmeasured | 26.10 | unmeasured | 25.64 |
| Engine CPU time, s | unmeasured | 102.422 | unmeasured | 115.563 |

Engine CPU and memory refer to the test process lifetime, including fixture setup
and both previews; CPU seconds are not wall-clock time. x86 unchanged preview was
0.54 s slower in this sample (~8%): flag for an isolated repeat before release;
no improvement is claimed for that phase or for unmeasured workloads.
Raw outputs: `qa/perf-{x64,x86}-{before,after}.txt`.

## Visible idle GUI

Reproduce with `scripts/measure-idle.ps1 -Architecture x64 -Label after` (or x86).
An isolated empty GUI warms up for 5 s, then its own process and WebView descendants
are sampled for ~8 s. No backup jobs/network access. CPU is expressed as a fraction
of ONE core, not Task Manager's whole-machine percentage.

| Summed process-tree observation | x64 before | x64 after | x86 before | x86 after |
|---|---:|---:|---:|---:|
| CPU ms in sample | 62.500 | 62.500 | 15.625 | 46.875 |
| One-core CPU % | 0.758 | 0.779 | 0.193 | 0.586 |
| Peak summed working sets, MiB | 327.31 | 324.02 | 314.53 | 313.87 |
| Peak summed private commit, MiB | 189.47 | 183.04 | 180.50 | 176.07 |

All samples contained seven processes. Shared pages can be counted repeatedly in
summed working sets; private commit is not physical RAM. Short CPU samples have
coarse timer granularity and startup/cache variability. No meaningful idle memory
or CPU reduction is claimed. WebView dominates the visible GUI footprint.
Raw machine/sample metadata: `qa/perf-idle-*.json`.

Temporary tray host (`test-activity-host.py --measure`): x64 ~29.76 MiB working set,
x86 ~29.24 MiB; both sampled 0 CPU ms over ~2.3 s (below measurement resolution,
not proof of literally zero CPU). It exists during work, not as an idle service.
Hidden-after-copy GUI exits, verified by native lifecycle tests for both builds.

Provisional review budgets on this fixture/host: empty GUI private commit <240 MiB,
idle <1.5% of one core in this 8 s scenario, engine peak working set <40 MiB on
10k files. These are regression review thresholds, not application resource/speed
limits or promises for every machine. Track timing separately under stable load.

## Validation and remaining gates

Both release GUIs/CLIs built; 53 Rust tests passed per architecture (4 opt-in tests
skipped), 15 Node tests passed. Clippy/fmt/TypeScript passed. The real GUI copied
512 MiB with hash verification while hidden, reopened repeatedly, cancelled safely
and preserved an open window. Native OS scheduler tests passed for both CLIs.

Large-file throughput/peak RAM during that GUI scenario were not sampled; it is a
functional regression check. Many simultaneously runnable jobs, disconnected real
NAS, removable hardware, long idle samples, native x86 Windows and Linux/macOS
remain separate performance/release gates. Do not infer their results from these
local tests. See VALIDATION.md and Defender reports for performed checks.
