# Approved Dark Minimal

The local development workspace contains a user-supplied visual reference and
QA screenshots. They are not included in the public source export: reference
redistribution rights have not been verified and QA images can contain local
paths. Application icons in apps/desktop/src-tauri/icons are retained for builds.

Use six primary navigation destinations: Home, Jobs, Schedule, Archive, History
and Settings. Preserve compact folder rows, status icons with text, blue primary
actions and the near-black sidebar. Prefer flat surfaces and accessible controls.

Tokens: sidebar #101722, canvas #151B25, panel #1D2632, border #2C3644,
primary #0866E9, text #F5F7FA, muted #A6B0BF. Focus must be visible; validate
contrast on actual controls. Use system fonts, no remote font requests.

The first launch has no jobs, capacity figures or successful run claims.
Its primary action is «Добавить задачу» / “Add job”. Once jobs exist, use one
«Синхронизировать сейчас» / “Run now” action, with explicit job scope.
Capacity is measured for the selected local volume or mounted share; unknown
values stay unknown. Archive capacity is not an invented quota.

Before sign-off: compare at 1536×1024; verify 1024×700 and Windows scaling
125%/150%, keyboard access, error announcements, RU/EN overflow, reduced motion.
No UI or visual sign-off has occurred in this stage.

Tool discovery, 2026-10-19:
- UI UX Pro Max skill read; initial design-system search recommended an
  unrelated marketing layout/light palette, rejected in favor of the reference.
  A narrower retry also suggested a marketing layout/glassmorphism and was rejected.
  No suitable design-system match was verified; the approved reference and AGENTS.md
  remain authoritative. Targeted UX search did verify error-summary focus, linked
  inline validation and announced errors; apply these to the future create/edit form.
- shadcn MCP `view_items_in_registries` succeeded for @shadcn/button and dialog.
- No components installed or altered; component CLI/docs workflow is pending UI work.
