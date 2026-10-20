# Implemented IPC update — 2026-10-19

`backup_command` receives the tagged `Request` enum from
`apps/desktop/src-tauri/src/commands.rs`. Camel-case actions: listJobs, archiveJobs,
saveJob, preview, run, operations, archive, restore, cleanup, history, pause,
remove, capacity, schedule and nextRun. Each handler runs on a blocking worker.
Requests/results correspond to the Rust DTOs and frontend `src/api.ts`. Errors
are sanitized `{code, detail}`; restore-collision details are an opaque observed
file token, accepted only with a second explicit confirmation.

`backup-progress` emits actual committed-file counters at most every 200 ms,
plus initial/final events. No simulated percentages or idle polling.
`get_app_status` and `set_language` remain available. Real status now reports
executionAvailable=true. Native folder dialogs have a scoped capability.

The original separate-command proposal follows for design history. It is not a
list of exported command names; update the Request enum and frontend together.

# IPC contract v0 — proposed, not registered commands

Development-shell additions (implemented): `get_app_status() -> { version,
schemaVersion, jobCount, language, executionAvailable }`, and
`set_language({language: 'ru' | 'en'}) -> void`. Errors are sanitized string codes.
Status always reports `executionAvailable=false` until the engine is implemented.
These commands use blocking workers and the actual local SQLite database.
The backup commands below remain proposed and are not registered in Tauri.

Use a versioned Rust/Serde contract and derive/export TypeScript types before UI
implementation. The core types in this scaffold do not yet serialize. Never keep
two independently maintained definitions of an active command schema.

| Command | Input | Result / policy |
| --- | --- | --- |
| list_jobs | cursor, page_size | bounded page with continuation cursor |
| validate_job | draft + editing job ID | field errors, resolved paths/type, live identity |
| save_job | draft, expected_revision | saved disabled job; editing invalidates preview consent |
| preview_job | job ID, revision | run ID, complete-scan summary, warnings; paged operations |
| approve_preview | run ID, revision, explicit deletion consent | persisted approval bound to revision/identity |
| run_job | job ID, approved revision | real run ID; rejects stale approval or busy resources |
| set_job_enabled | job ID, revision, enabled | stored state and actual scheduler registration result |
| remove_job | job ID, revision, confirmation | unregister + soft delete; archive remains recoverable |
| list_archive | job ID, cursor, page_size | original path, deletion/replacement/expiry timestamps |
| restore_version | version ID, optional collision approval | never overwrites silently; revalidates identities |
| list_history | job/error filter, cursor, page_size | sanitized run details |
| destination_capacity | job ID | measured bytes or explicit unavailable reason |
| set_language | ru or en | persisted language |

Common error envelope: code, localization parameters, safe context, retryability.
Distinguish missing source, incomplete scan, disconnected/wrong destination, busy,
archive failure, mass deletion, expired preview, restore collision and recovery.
Log paths only when diagnostically needed; never log credentials or raw UNC URLs
containing secrets. Do not invoke commands using caller-supplied shell fragments.

Real progress events: run ID, phase, counts/bytes completed, optional total only
when known; bounded frequency. Late subscribers read current persisted state.
Cancel is acknowledged as canceled/interrupted, never success. Tauri commands
dispatch heavy operations to workers, coalesce duplicate triggers and return promptly.

Native folder browsing: Tauri dialog plugin, scoped capabilities, explicit source
and destination choices. Manual input follows exactly the same validation path.
Network destinations require OS-mounted shares; local-only operation requires no
NAS connection. UI errors use RU/EN resource keys, not backend English sentences.


Implemented activity contract (2026-10-19): backup_command {action:"activity"}
returns at most 50 live records: jobId, name, phase, completed, total, copiedBytes,
totalBytes, currentPath, updatedAt. The backup-progress event has the same execution
fields except name/updatedAt, which are supplied by the job list/local snapshot.
get_app_status includes notificationsEnabled; set_notifications {enabled:boolean}
persists the shared setting. Job list records include lastError for neutral stale
preview presentation. run still returns result + separate scheduleError.

Migration 10 activity records/events add verifiedBytes and etaSeconds (nullable).
The local worker calculates ETA so a newly opened GUI reads the current estimate.
phases include flushing and verifying; totalBytes denotes copy bytes, while the
progress percentage combines copiedBytes + verifiedBytes over twice totalBytes.
Finalization remains active (at most 99%) until core execution succeeds.
