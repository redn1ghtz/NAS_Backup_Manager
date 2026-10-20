"""Prepare reviewed source and release-asset folders locally; never publish or delete.

Python 3.11+, Cargo's populated offline cache, installed npm dependencies for
available license texts, built alpha binaries, and Inno Setup 7 are required.
"""
from __future__ import annotations

import argparse
from functools import cache
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.0-alpha"
OMIT_DIRS = {".git", ".codex", ".agents", ".aws", "node_modules", "target", "dist", "gen", "__pycache__", "graphify-out", "data"}
OMIT_SUFFIXES = {".exe", ".dll", ".zip", ".7z", ".msi", ".sqlite", ".sqlite3", ".db", ".log", ".pyc", ".pem", ".pfx", ".p12", ".key"}


def sha(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def license_files(directory: Path) -> list[Path]:
    if not directory.is_dir():
        return []
    found = []
    for entry in directory.iterdir():
        if entry.is_file() and not entry.is_symlink() and entry.name.lower().startswith(("license", "licence", "copying", "notice", "copyright", "unlicense")):
            if entry.stat().st_size < 1_000_000:
                found.append(entry)
        elif entry.is_dir() and not entry.is_symlink() and entry.name.lower() in {"licenses", "licences"}:
            found.extend(p for p in entry.iterdir() if p.is_file() and not p.is_symlink() and p.stat().st_size < 1_000_000)
    return sorted(found)


def collect_licenses() -> tuple[Path, dict]:
    packages = {}
    for architecture in ("x86_64", "i686"):
        command = ["cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", f"{architecture}-pc-windows-msvc"]
        result = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, encoding="utf-8")
        for package in json.loads(result.stdout)["packages"]:
            packages[(package["name"], package["version"])] = package
    snapshot = ROOT / "target/third-party-licenses"
    snapshot.mkdir(parents=True, exist_ok=True)
    rows = []
    for (name, version), package in sorted(packages.items()):
        directory = Path(package["manifest_path"]).parent
        sources = license_files(directory)
        if not package.get("source"):
            sources = [ROOT / "LICENSE"]
        declared = package.get("license") or package.get("license_file") or "NOT DECLARED"
        row = copy_notices(snapshot, "rust", name, version, declared, sources)
        row["upstream"] = package.get("repository")
        row["sourceArchive"] = f"https://crates.io/api/v1/crates/{name}/{version}/download" if package.get("source") else "https://github.com/redn1ghtz/NAS_Backup_Manager"
        rows.append(row)
    npm = json.loads((ROOT / "apps/desktop/package-lock.json").read_text(encoding="utf-8"))
    for location, package in sorted(npm["packages"].items()):
        if not location:
            continue
        name = package.get("name") or location.rsplit("node_modules/", 1)[-1]
        directory = ROOT / "apps/desktop" / location
        row = copy_notices(snapshot, "npm", name, package["version"], package.get("license", "NOT DECLARED"), license_files(directory))
        row["sourceArchive"] = package.get("resolved")
        rows.append(row)
    if any(row["declaredLicense"] == "NOT DECLARED" for row in rows):
        raise RuntimeError("Missing declared license metadata; review before publication")
    inventory = {"version": VERSION, "scope": "Resolved Rust Windows x64/x86 dependency union and complete npm lock metadata, including development/optional packages", "cargoLockSha256": sha(ROOT / "Cargo.lock"), "npmLockSha256": sha(ROOT / "apps/desktop/package-lock.json"), "packages": rows}
    write_json(snapshot / "inventory.json", inventory)
    available = sum(bool(row["noticeFiles"]) for row in rows)
    readme = f"""# Dependency license snapshot — {VERSION}

{len(rows)} package/version records; {available} have locally available notice files.
Declared license metadata is copied verbatim from Cargo metadata/package-lock.json.
Optional npm packages absent on this host may have metadata only. This is not a
complete legal/compliance audit or a claim that every package is linked at runtime.
Review exact dependency obligations, included source/NOTICE requirements, platform
runtime terms and THIRD_PARTY_NOTICES.md before public distribution. No Windows,
WebView2 or .NET system binaries are redistributed in this snapshot.

Builds use the two lockfile SHA-256 values recorded in inventory.json. Original
project crates use the root MIT notice. The source export omits supplied design
reference/QA screenshots and generated graph; build icons remain included.
"""
    (snapshot / "README.md").write_text(readme, encoding="utf-8")
    expected = {"README.md", "inventory.json"} | {p for row in rows for p in row["noticeFiles"]}
    extra = {p.relative_to(snapshot).as_posix() for p in snapshot.rglob("*") if p.is_file()} - expected
    if extra:
        raise RuntimeError("Unexpected files in the managed license snapshot; inspect locally before packaging: " + ", ".join(sorted(extra)[:5]))
    lines = [readme, "\n| Ecosystem | Package | Version | Declared license | Local notice files |", "|---|---|---|---|---|"]
    lines.extend(f"| {r['ecosystem']} | {r['name']} | {r['version']} | {str(r['declaredLicense']).replace('|', '/')} | {len(r['noticeFiles'])} |" for r in rows)
    (ROOT / "docs/DEPENDENCY_LICENSES.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return snapshot, {"records": len(rows), "recordsWithLocalNotices": available, "metadataOnlyRecords": len(rows) - available}


@cache
def supplements() -> dict:
    path = ROOT / "docs/third-party/notices/index.json"
    return {(r["ecosystem"], r["name"], r["version"]): r for r in json.loads(path.read_text())} if path.exists() else {}


def copy_notices(snapshot: Path, ecosystem: str, name: str, version: str, declared: str, sources: list[Path]) -> dict:
    supplement = supplements().get((ecosystem, name, version))
    if supplement:
        for notice in supplement["notices"]:
            path = (ROOT / notice["file"]).resolve()
            path.relative_to((ROOT / "docs/third-party/notices").resolve())
            if sha(path) != notice["sha256"]:
                raise RuntimeError(f"Notice supplement hash mismatch: {name}")
            sources.append(path)
    slug = re.sub(r"[^a-zA-Z0-9_.-]", "_", name) + "-" + version
    notices = []
    for source in sources:
        destination = snapshot / ecosystem / slug / source.name
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists() and destination.read_bytes() != source.read_bytes():
            destination = destination.with_name(source.parent.name + "-" + source.name)
        shutil.copyfile(source, destination)
        notices.append(destination.relative_to(snapshot).as_posix())
    row = {"ecosystem": ecosystem, "name": name, "version": version, "declaredLicense": declared, "noticeFiles": notices}
    if supplement:
        row["noticeProvenance"] = supplement
    return row


def copy_source(destination: Path) -> list[str]:
    files = []
    roots = ["README.md", "LICENSE", "THIRD_PARTY_NOTICES.md", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".gitignore", ".gitattributes", "CHANGELOG.md", "CONTRIBUTING.md", "SECURITY.md"]
    candidates = [ROOT / name for name in roots]
    for name in ("apps", "crates", "installers", "scripts", "tests", "docs"):
        for base, directories, names in os.walk(ROOT / name):
            directories[:] = sorted(d for d in directories if d not in OMIT_DIRS and not d.startswith(".graphify") and not (Path(base) / d).is_symlink())
            candidates.extend(Path(base) / filename for filename in sorted(names))
    for source in candidates:
        relative = source.relative_to(ROOT)
        if source.is_symlink() or source.suffix.lower() in OMIT_SUFFIXES or source.name.startswith((".env", ".graphify_")):
            continue
        if source.suffix.lower() == ".png" and relative.parts[:2] in {("docs", "qa"), ("docs", "design")}:
            continue
        if source.name == ".DS_Store" or re.search(r"\.(?:sqlite|db)(?:-|\.)", source.name):
            continue
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        files.append(relative.as_posix())
    return files


def zip_paths(output: Path, files: list[tuple[Path, str]]) -> None:
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for source, name in sorted(files, key=lambda value: value[1]):
            if source.is_symlink():
                raise RuntimeError(f"Unexpected symlink: {source.name}")
            archive.write(source, name)
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None:
            raise RuntimeError(f"Archive verification failed: {output.name}")


def main() -> None:
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    output = (args.output_dir or ROOT / "github-upload" / VERSION).resolve()
    publication_root = (ROOT / "github-upload").resolve()
    publication_root.relative_to(ROOT)
    output.relative_to(publication_root)
    if output.exists():
        raise RuntimeError("Output already exists; choose a new empty --output-dir under github-upload. Nothing was deleted.")
    package = json.loads((ROOT / "apps/desktop/package.json").read_text())
    if package["version"] != VERSION:
        raise RuntimeError("Update the publication helper for the current version")
    licenses, license_summary = collect_licenses()
    # Repackage existing binaries with notices, then rebuild only Inno installers.
    for script, extra in (("package-release.ps1", ["-SkipBuild"]), ("build-installers.ps1", [])):
        subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(ROOT / "scripts" / script), *extra], cwd=ROOT, check=True)
    release = ROOT / "releases" / VERSION
    output.mkdir(parents=True)
    source = output / "repository"
    source.mkdir()
    copied = copy_source(source)
    assets = output / "release-assets"
    assets.mkdir()
    base = f"NAS-Backup-Manager-{VERSION}"
    for arch in ("x64", "x86"):
        portable = release / f"portable-{arch}"
        allowed = {"NAS Backup Manager.exe", "nas-backup.exe", "nas-backup-notify.exe", "portable.flag", "LICENSE", "THIRD_PARTY_NOTICES.md", "README.md", "USER_GUIDE.md", "VALIDATION.md", "INSTALLER.md"}
        files = [(portable / name, f"portable-{arch}/{name}") for name in sorted(allowed)]
        files.extend((path, f"portable-{arch}/licenses/" + path.relative_to(licenses).as_posix()) for path in licenses.rglob("*") if path.is_file())
        zip_paths(assets / f"{base}-portable-{arch}.zip", files)
        setup = release / "installers" / f"{base}-setup-{arch}.exe"
        shutil.copyfile(setup, assets / setup.name)
    zip_paths(assets / f"{base}-inno-scripts.zip", [(release / name, name) for name in ("setup-x64.iss", "setup-x86.iss", "setup-common.iss", "BUILD_INSTALLERS.md")] + [(ROOT / "LICENSE", "LICENSE")])
    zip_paths(assets / f"{base}-third-party-licenses.zip", [(p, p.relative_to(licenses).as_posix()) for p in licenses.rglob("*") if p.is_file()])
    hashes = [f"{sha(p)}  {p.name}" for p in sorted(assets.iterdir())]
    (assets / "SHA256SUMS.txt").write_text("\n".join(hashes) + "\n", encoding="utf-8")
    shutil.copyfile(ROOT / "docs/RELEASE_NOTES_0.1.0-alpha.md", output / "RELEASE_NOTES.md")
    shutil.copyfile(ROOT / "docs/GITHUB_PUBLISH.md", output / "PUBLISH_GITHUB_RU.md")
    secret_pattern = re.compile(rb"(?:gh[pousr]_[a-zA-Z0-9]{25,}|github_pat_[a-zA-Z0-9_]{25,}|AKIA[0-9A-Z]{16}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)")
    for path in source.rglob("*"):
        if path.is_file() and secret_pattern.search(path.read_bytes()):
            raise RuntimeError(f"Possible credential/private key in {path.relative_to(source)}; inspect before publishing")
    report = {"version": VERSION, "sourceFiles": len(copied), "sourceBytes": sum((source / name).stat().st_size for name in copied), "sourcePaths": copied, "assets": [{"name": p.name, "bytes": p.stat().st_size, "sha256": sha(p)} for p in sorted(assets.iterdir())], "licenseSnapshot": license_summary, "excluded": ["target/node_modules/dist", "portable user state and databases", "local agent instructions/settings", "QA and supplied reference PNGs", "generated Graphify artifacts"], "audit": "Allowlisted source; no symlinks, runtime EXE/DLL/archives or private state; basic credential-pattern scan passed. Not a complete secret/compliance audit.", "published": False}
    write_json(output / "PUBLICATION_REPORT.json", report)
    print(f"Prepared: {output}\nSource files: {len(copied)}; release assets: {len(list(assets.iterdir()))}\nNothing uploaded to GitHub.")


if __name__ == "__main__":
    main()
