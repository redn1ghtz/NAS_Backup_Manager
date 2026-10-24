# Corresponding source for bundled Go tools — 0.2.0-alpha

Release: https://github.com/redn1ghtz/NAS_Backup_Manager/releases/tag/v0.2.0-alpha

Download `NAS-Backup-Manager-0.2.0-alpha-tool-sources.zip` from that release.
It supplies the source module ZIPs for the 248 exact modules recorded in the
Windows x64/x86 age, age-keygen and rclone binaries, including their main modules.
No user credentials, user cloud accounts or user private recovery keys are included.
Upstream source archives can contain their own public test fixtures/example keys.
`notices/modules/inventory.json` identifies versions, verified Go h1 checksums,
upstream source URLs and notice file hashes. The archive's `proxy/` tree is a Go
file module proxy; ZIPs contain original source, module files and included notices.

age 1.3.2 is BSD-3-Clause; rclone 1.75.2 is MIT. Their dependencies have separate
licenses. In particular `github.com/cloudsoda/sddl` is LGPL-3.0 and some modules
are MPL-2.0. License/NOTICE files are included in `tools/notices/` in each binary
package and `notices/` in the source archive. This source is supplied alongside
the executable distribution, rather than merely promised on request.

## Rebuild or replace a component

Use Go compatible with each module's go.mod (bundled rclone was built with
Go 1.26.9; x64 age with its upstream toolchain, x86 age with Go 1.27.3). The Go
compiler is not bundled. Get it from https://go.dev/dl/ and verify its checksum.
Read build metadata using `go version -m path/to/rclone.exe`.

1. Extract this source archive. Find the main module ZIP under
   `proxy/github.com/rclone/rclone/@v/v1.75.2.zip` (age uses
   `proxy/filippo.io/age/@v/v1.3.2.zip`). Extract that ZIP into a writable source
   directory; its root contains go.mod, go.sum and source/build files.
2. In PowerShell set `$env:GOPROXY='file:///C:/path/to/extracted/proxy,https://proxy.golang.org'`.
   The first location supplies included sources; Go may fetch additional build
   graph metadata from upstream. Preserve go.mod and go.sum.
3. For a modified LGPL library, extract its included ZIP from
   `proxy/github.com/cloudsoda/sddl/@v/` to a separate directory, edit its source,
   then run in rclone's source root:
   `go mod edit -replace github.com/cloudsoda/sddl=C:/path/to/modified/sddl`.
   Other module dependencies can be replaced in the same way.
4. Set `$env:GOOS='windows'`, `$env:GOARCH='amd64'` (or `'386'` for x86),
   `$env:CGO_ENABLED='0'`; from rclone's source root run
   `go build -trimpath -o rclone.exe .`.
   For age, from age's source root run
   `go build -trimpath -o age.exe ./cmd/age` and
   `go build -trimpath -o age-keygen.exe ./cmd/age-keygen`.
5. Close the application and finish all backup work, then replace the corresponding
   EXE under its `tools/` directory. No signature/activation is required; keep
   matching architecture and command-line compatibility. Check your modified
   binary on disposable folders before using valuable backups.

These are functional rebuilding instructions, not a promise of byte-identical
upstream binaries. Keep component copyright/license notices with redistribution.
Do not use upstream names to imply endorsement. The original application code
is independent MIT-licensed code; the tools are separate command-line programs.
