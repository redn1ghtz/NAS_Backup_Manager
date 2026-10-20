# Desktop app

Tauri 2 + React + TypeScript + Vite + Tailwind CSS + shadcn/ui.
Use npm/package-lock.json and the root Cargo.lock; do not switch package managers.

`npm ci`, `npm run typecheck`, `npm run build`, `npm run tauri -- dev`.
For the requested ordinary test folder, run `scripts/package-portable.ps1`
from the repository root. It builds the UI, native shell and headless CLI,
then updates executable files without deleting the folder's `data/`.

All six navigation destinations use actual state/commands. Home/jobs, guided
configuration, paged preview, archive/restore/expiry, history filtering, schedule
registration and settings are connected. Secondary views are lazy-loaded; lists
are paginated. No idle filesystem/status polling. Progress events are throttled.

The typed `backup_command` request enum in `src-tauri/src/commands.rs` calls the
shared core. Database/filesystem work runs on blocking workers. Native folder
browsing uses the scoped Tauri dialog plugin. Browser mode is a view-only shell
and explicitly reports that local state is unavailable.

No development-version banner or fabricated backup success/capacity values.
See `docs/VALIDATION.md` and `docs/USER_GUIDE.md` for tested limits and release gates.
