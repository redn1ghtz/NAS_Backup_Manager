"""Collect notices from exact Go modules linked into bundled tools (build-time only).

Uses Go's module proxy/checksum verification and verifies the h1 recorded in each
binary. Downloads stay in target/optional-tools; no user config is read.
"""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import os
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / 'target/optional-tools'
GO = CACHE / 'go-toolchain/go/bin/go.exe'
ENV = dict(os.environ, GOPATH=str(CACHE / 'go-work'), GOCACHE=str(CACHE / 'go-cache'), GOTOOLCHAIN='local')

def run(*args):
    return subprocess.check_output([str(GO), *args], cwd=ROOT, env=ENV, text=True, encoding='utf-8')

def main():
    modules = {}
    for arch in ('x64', 'x86'):
        for tool in ('rclone', 'age', 'age-keygen'):
            text = run('version', '-m', str(CACHE / arch / (tool + '.exe')))
            for line in text.splitlines():
                values = line.split()
                if values and values[0] in ('mod', 'dep') and len(values) >= 3:
                    path, version = values[1:3]
                    checksum = values[3] if len(values) > 3 else ''
                    modules[(path, version)] = checksum
                if values and values[0] == '=>':
                    raise RuntimeError('Module replacement needs manual review')
    destination = CACHE / 'module-notices'
    destination.mkdir(exist_ok=True)

    def collect(item):
        (module, version), expected = item
        metadata = json.loads(run('mod', 'download', '-json', module + '@' + version))
        if expected and metadata.get('Sum') != expected:
            raise RuntimeError('Module checksum mismatch: ' + module)
        source = Path(metadata['Dir'])
        slug = re.sub(r'[^A-Za-z0-9_.-]', '_', module + '@' + version)
        files = []
        for path in source.rglob('*'):
            if path.is_file() and not path.is_symlink() and path.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'copyright', 'unlicense')):
                if path.stat().st_size > 2_000_000:
                    raise RuntimeError('Oversized notice: ' + module)
                relative = Path(slug) / path.relative_to(source)
                target = destination / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
                files.append({'path': relative.as_posix(), 'sha256': hashlib.sha256(target.read_bytes()).hexdigest()})
        if not files:
            raise RuntimeError('No notices in module: ' + module)
        escaped = ''.join('!' + c.lower() if c.isupper() else c for c in module)
        return {'module': module, 'version': version, 'checksum': metadata['Sum'], 'sourceArchive': f'https://proxy.golang.org/{escaped}/@v/{version}.zip', 'notices': files}

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        rows = sorted(pool.map(collect, sorted(modules.items())), key=lambda row: row['module'])
    (destination / 'inventory.json').write_text(json.dumps({'modules': rows, 'scope': 'Go build-info union of bundled x64/x86 age, age-keygen and rclone; exact module checksums verified. License texts collected, not an automated legal assessment.'}, indent=2) + '\n', encoding='utf-8')
    for arch in ('x64', 'x86'):
        shutil.copytree(destination, CACHE / arch / 'notices/modules', dirs_exist_ok=True)
    gpl = CACHE / 'GPL-3.0.txt'
    if not gpl.exists():
        urllib.request.urlretrieve('https://www.gnu.org/licenses/gpl-3.0.txt', gpl)
    if hashlib.sha256(gpl.read_bytes()).hexdigest() != '3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986':
        raise RuntimeError('GPL text checksum mismatch')
    for arch in ('x64', 'x86'):
        shutil.copyfile(gpl, CACHE / arch / 'notices/GPL-3.0.txt')
    print(f'Collected notices for {len(rows)} exact Go modules; copied to both tool packages.')

if __name__ == '__main__':
    main()
