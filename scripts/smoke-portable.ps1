$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$smokeDirectory = Join-Path (Join-Path $projectRoot 'target') ('portable-smoke-' + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $smokeDirectory | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'portable/NAS Backup Manager/NAS Backup Manager.exe') -Destination $smokeDirectory
Copy-Item -LiteralPath (Join-Path $projectRoot 'portable/NAS Backup Manager/nas-backup.exe') -Destination $smokeDirectory
Copy-Item -LiteralPath (Join-Path $projectRoot 'portable/NAS Backup Manager/nas-backup-notify.exe') -Destination $smokeDirectory
Set-Content -LiteralPath (Join-Path $smokeDirectory 'portable.flag') -Value 'test-only' -Encoding utf8
$validationScript = Join-Path $smokeDirectory 'validate.py'
@'
import json, sqlite3, sys, time
for attempt in range(20):
    try:
        with sqlite3.connect(sys.argv[1]) as db:
            version = db.execute('SELECT max(version) FROM schema_migrations').fetchone()[0]
            integrity = db.execute('PRAGMA integrity_check').fetchone()[0]
            jobs = db.execute('SELECT count(*) FROM jobs').fetchone()[0]
            tables = {row[0] for row in db.execute('SELECT name FROM sqlite_master')}
        assert version == 10 and integrity == 'ok' and jobs == 0
        assert {'run_metrics', 'file_events'} <= tables
        print(json.dumps(dict(schema=version, integrity=integrity, jobs=jobs)))
        break
    except sqlite3.OperationalError:
        if attempt == 19:
            raise
        time.sleep(0.1)
'@ | Set-Content -LiteralPath $validationScript -Encoding utf8
$smokeProcess = Start-Process -FilePath (Join-Path $smokeDirectory 'NAS Backup Manager.exe') -WorkingDirectory $smokeDirectory -WindowStyle Hidden -PassThru
try {
    $smokeDatabase = Join-Path $smokeDirectory 'data/state.sqlite'
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        if (Test-Path -LiteralPath $smokeDatabase) { break }
        if ($smokeProcess.HasExited) { throw 'Packaged app exited before creating isolated state' }
        Start-Sleep -Milliseconds 250
    }
    if (-not (Test-Path -LiteralPath $smokeDatabase)) { throw 'Packaged app did not create isolated state' }
    & python $validationScript $smokeDatabase
    if ($LASTEXITCODE -ne 0) { throw 'Isolated state validation failed' }
    if ($smokeProcess.HasExited) { throw 'Packaged app exited during validation' }
    Write-Output "Packaged app stayed alive without a dev server. Isolated fixture: $smokeDirectory"
} finally {
    # Only stop the process created by this test; never touch an existing application.
    if (-not $smokeProcess.HasExited) { Stop-Process -Id $smokeProcess.Id }
}
