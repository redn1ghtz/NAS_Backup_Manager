# Opt-in native Inno smoke: unique application identity, directories and tasks only.
param([string]$ISCC='C:/Program Files/Inno Setup 7/ISCC.exe')
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$version=(Get-Content -LiteralPath (Join-Path $root 'apps/desktop/package.json') -Raw | ConvertFrom-Json).version
$release=Join-Path $root "releases/$version"
$fixture=Join-Path $root ('target/inno-smoke-'+[guid]::NewGuid())
New-Item -ItemType Directory -Path $fixture | Out-Null
$service=New-Object -ComObject 'Schedule.Service'; $service.Connect();$folder=$service.GetFolder('\')
$results=@()
function RegisterFixtureTask([string]$name,[string]$cli) {
    $task=$service.NewTask(0)
    $task.RegistrationInfo.Description='Disposable NAS Backup Manager installer QA'
    $task.Settings.Enabled=$true;$task.Settings.Hidden=$true
    $action=$task.Actions.Create(0);$action.Path=$cli;$action.Arguments='--help'
    $user=[Security.Principal.WindowsIdentity]::GetCurrent().Name
    $task.Principal.UserId=$user;$task.Principal.LogonType=3
    [void]$folder.RegisterTaskDefinition($name,$task,6,$user,$null,3)
}
function HasTask([string]$name) { try{[void]$folder.GetTask($name);return $true}catch{return $false} }
function RunUninstall([string]$directory,[string]$log) {
    $uninstaller=Join-Path $directory 'unins000.exe'
    $process=Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="'+$log+'"')) -WindowStyle Hidden -PassThru -Wait
    Start-Sleep -Milliseconds 300
    return $process.ExitCode
}
foreach($architecture in @('x64','x86')) {
  foreach($language in @('ru','en')) {
    $id=([guid]::NewGuid().ToString('N'))
    $directory=Join-Path $fixture "$architecture-$language"
    $compile=Join-Path $fixture "compiler-$id"
    & $ISCC ("/DQAId=$id") ("/O$compile") '/Ftest-setup' (Join-Path $release "setup-$architecture.iss") | Out-Null
    if($LASTEXITCODE -ne 0){throw 'QA installer compilation failed'}
    $installer=Join-Path $compile 'test-setup.exe'
    $process=Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LANG='+$language),('/DIR="'+$directory+'"'),('/LOG="'+(Join-Path $fixture "$id-install.log")+'"')) -WindowStyle Hidden -PassThru -Wait
    if($process.ExitCode -ne 0){throw "Install failed: $architecture/$language exit $($process.ExitCode)"}
    foreach($name in @('NAS Backup Manager.exe','nas-backup.exe','nas-backup-notify.exe','unins000.exe','tools/age.exe','tools/age-keygen.exe','tools/rclone.exe','tools/AGE-LICENSE','tools/RCLONE-COPYING','ADVANCED.md')) {
        if(-not(Test-Path -LiteralPath (Join-Path $directory $name))){throw "Missing installed file: $name"}
    }
    if(Test-Path -LiteralPath (Join-Path $directory 'portable.flag')){throw 'Installer included portable.flag'}
    foreach($tool in @('age.exe','age-keygen.exe','rclone.exe')) {
      if((Get-FileHash -LiteralPath (Join-Path $directory "tools/$tool")).Hash -ne (Get-FileHash -LiteralPath (Join-Path $release "portable-$architecture/tools/$tool")).Hash){throw "Tool payload mismatch: $tool"}
    }
    $own1='NASBackupManager-'+[guid]::NewGuid();$own2='NASBackupManager-'+[guid]::NewGuid()
    $foreign='NASBackupManager-'+[guid]::NewGuid();$unrelated='NASBackupInstallerQA-'+[guid]::NewGuid()
    $cli=Join-Path $directory 'nas-backup.exe'
    $otherCli=Join-Path $fixture "other-$id.exe"
    Copy-Item -LiteralPath $cli -Destination $otherCli
    $names=@($own1,$own2,$foreign,$unrelated)
    try {
      RegisterFixtureTask $own1 $cli;RegisterFixtureTask $own2 $cli
      RegisterFixtureTask $foreign $otherCli;RegisterFixtureTask $unrelated $cli
      $state=Join-Path $directory 'qa-state.sqlite'
      # Actual migrations, private database. No real configuration is opened.
      $fixtureScript=@'
import sqlite3,sys,pathlib,uuid
db=sqlite3.connect(sys.argv[1]);root=pathlib.Path(sys.argv[2])
for p in sorted((root/'crates/storage/migrations').glob('*.sql')):db.executescript(p.read_text())
db.execute("INSERT INTO jobs(id,name,source_path,destination_path,destination_kind,expected_identity,ownership_nonce,enabled,approved_revision,schedule_enabled,schedule_automatic,created_at,updated_at) VALUES(?, 'Installer QA','source','destination','local','fixture','fixture',1,1,1,1,0,0)",(str(uuid.uuid4()),))
db.commit()
'@
      $fixtureScript | python - $state $root
      if($LASTEXITCODE -ne 0){throw 'Could not create QA database'}
      Set-Content -LiteralPath (Join-Path $directory 'keep-user-file.txt') -Value 'Must survive uninstall'
      if($architecture -eq 'x64' -and $language -eq 'ru') {
        # A metadata failure must stop before deleting application files/tasks.
        'import sqlite3,sys;d=sqlite3.connect(sys.argv[1]);d.execute("INSERT INTO schema_migrations VALUES(999,0)");d.commit()' | python - $state
        if($LASTEXITCODE -ne 0){throw 'Could not create future-schema fixture'}
        [void](RunUninstall $directory (Join-Path $fixture "$id-rejected.log"))
        if(-not(Test-Path -LiteralPath $cli) -or -not(HasTask $own1)){throw 'Failed cleanup did not preserve app/tasks'}
        'import sqlite3,sys;d=sqlite3.connect(sys.argv[1]);d.execute("DELETE FROM schema_migrations WHERE version=999");d.commit()' | python - $state
        if($LASTEXITCODE -ne 0){throw 'Could not reset future-schema fixture'}
      }
      $code=RunUninstall $directory (Join-Path $fixture "$id-uninstall.log")
      if($code -ne 0){throw "Uninstall failed: $code"}
      if((HasTask $own1) -or (HasTask $own2)){throw 'Own scheduled tasks survived uninstall'}
      if(-not(HasTask $foreign) -or -not(HasTask $unrelated)){throw 'Uninstall touched unrelated scheduled tasks'}
      if(Test-Path -LiteralPath $cli){throw 'Installed executable survived successful uninstall'}
      if(-not(Test-Path -LiteralPath $state) -or -not(Test-Path -LiteralPath (Join-Path $directory 'keep-user-file.txt'))){throw 'Uninstall removed user data'}
      $enabled='import sqlite3,sys;d=sqlite3.connect(sys.argv[1]);print(d.execute("SELECT sum(schedule_enabled) FROM jobs").fetchone()[0])' | python - $state
      if($LASTEXITCODE -ne 0){throw 'Could not verify schedule metadata'}
      if($enabled.Trim() -ne '0'){throw 'Retained configuration still marks schedules enabled'}
      $results += [pscustomobject]@{architecture=$architecture;language=$language;installed=$true;ownTasksRemoved=$true;foreignTasksPreserved=$true;userDataPreserved=$true;scheduleFlagsDisabled=$true}
      Write-Output "PASS $architecture/$language install + uninstall; owned tasks removed; unrelated tasks/data preserved."
    } finally {
      foreach($name in $names){if(HasTask $name){$folder.DeleteTask($name,0)}}
    }
  }
}
$results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $root 'docs/qa/installer-smoke.json') -Encoding utf8
Write-Output "Disposable evidence: $fixture"
