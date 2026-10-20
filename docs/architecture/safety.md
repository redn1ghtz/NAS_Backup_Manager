# Safety implementation and test gates

Implementation now covers full source scans before mutation, persisted volume/directory
identities and owner markers, overlap/root/link guards, cross-process job locks,
stale preview checks, staged verified writes, archive-before-replace/remove,
collision-bound restore, expiry ownership/origin-run checks, and interrupted-run
reconciliation. The gates below remain the release checklist, not a claim that
all fault points or hostile path-swap races were exhaustively verified.

1. Resolve live source and destination handles; reject roots, home/system folders,
   aliases, symlinks/reparse points/junctions until safely supported, all source ↔
   destination/archive overlaps and cross-job destination overlaps. Compare actual
   canonical filesystem identities, including case-insensitive Windows paths.
2. Verify expected OS volume identity or mounted server/share identity and ownership
   marker; marker alone is insufficient. Recheck before mutations and after reconnect.
   Unsupported filesystems/identity mechanisms fail closed. Never create missing
   mount roots or infer that a missing source is an empty source.
3. Lock resources across UI/CLI processes with crash-released OS locks. SQLite's
   unique active-run constraint alone cannot prevent cross-job path races.
4. Complete the source scan, including unreadable children, before planning removals.
   Detect source identity changes, stale metadata and suspicious mass deletions;
   require a fresh preview and explicit consent. Zero-byte source is not proof of loss.
5. Revalidate each action against its preview metadata; refuse stale plans. Flush
   stage files, verify integrity, record recovery state. Before replacement/removal,
   copy the old destination to a unique archive version, flush and verify it.
   On failure preserve the old active file. Platform rename semantics need adapters.
6. Do not report success until file commits and metadata agree. On startup reconcile
   interrupted states conservatively; don't garbage-collect the only valid copy.
7. Restore via the same confined path/identity/ownership checks and locks. Collision
   approval binds to observed target identity; archive existing target before overwrite.
8. Retention is a separate job: verified expiry, verified ownership, successful run,
   no pending restore and no path escape. Failed/incomplete runs cannot expire data.
   Never follow links or delete active files; do not silently purge on job removal.
9. Warn about open/locked databases; filesystem copies are not application-consistent
   snapshots. File sharing, disk full and transient disconnects are visible errors.

Write deterministic tests before enabling mutations. Required fault points include
scan, archive allocation/write/flush/verification, staging copy/flush, rename, metadata
commit, process death and retention deletion. All tests use disposable data only.
Include case aliases, long/Unicode paths, traversal, mount swaps, repeated versions,
archive failure keeping active files, interrupted restore and manual/scheduled races.
