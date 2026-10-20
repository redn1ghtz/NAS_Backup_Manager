# Opt-in native activation test. Only a UUID-named clone is opened/stopped.
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$realApp=Join-Path $root 'portable/NAS Backup Manager/NAS Backup Manager.exe'
$package=if($env:NAS_BACKUP_TEST_PACKAGE){$env:NAS_BACKUP_TEST_PACKAGE}else{Split-Path $realApp}
$packageApp=Join-Path $package 'NAS Backup Manager.exe'
$testDirectory=Join-Path $root ('target/activation-smoke-'+[guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $testDirectory | Out-Null
$testApp=Join-Path $testDirectory 'NAS Backup Manager.exe'
Copy-Item -LiteralPath $packageApp -Destination $testApp
Copy-Item -LiteralPath (Join-Path $package 'nas-backup-notify.exe') -Destination $testDirectory
Set-Content -LiteralPath (Join-Path $testDirectory 'portable.flag') -Value 'disposable activation QA' -Encoding utf8
$hostFile=Get-Item -LiteralPath (Join-Path $package 'nas-backup-notify.exe')
if(-not $hostFile){throw 'Build the storage crate first'}
function NativeAction([string]$action,[string]$application){
 $native=Start-Process -FilePath $hostFile.FullName -ArgumentList @($action,('"'+$application+'"')) -WindowStyle Hidden -Wait -PassThru
 if($native.ExitCode -ne 0){throw "Native action failed: $action"}
}
function TestWindows { @(Get-Process -Name 'NAS Backup Manager' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $testApp -and $_.MainWindowHandle -ne 0 }) }
try{
 NativeAction '--register' $testApp
 $shortcut=Join-Path ([Environment]::GetFolderPath('Programs')) 'NAS Backup Manager.lnk'
 $identityTime=(Get-Item -LiteralPath $shortcut).LastWriteTimeUtc
 $identityHash=(Get-FileHash -LiteralPath $shortcut -Algorithm SHA256).Hash
 Start-Sleep -Milliseconds 200
 NativeAction '--register' $testApp
 if((Get-Item -LiteralPath $shortcut).LastWriteTimeUtc -ne $identityTime -or (Get-FileHash -LiteralPath $shortcut -Algorithm SHA256).Hash -ne $identityHash){throw 'Repeated setup rewrote the notification shortcut'}
 Start-Process 'nasbackupmanager://open' -WindowStyle Hidden | Out-Null
 $deadline=[DateTime]::UtcNow.AddSeconds(20)
 do{
  $windows=TestWindows
  if($windows.Count -eq 1){break}
  Start-Sleep -Milliseconds 250
 }while([DateTime]::UtcNow -lt $deadline)
 if($windows.Count -ne 1){throw 'Protocol did not open the disposable GUI'}
 $originalPid=$windows[0].Id
 NativeAction '--open' $testApp
 Start-Process 'nasbackupmanager://open' -WindowStyle Hidden | Out-Null
 Start-Sleep -Seconds 2
 $windows=TestWindows
 if($windows.Count -ne 1 -or $windows[0].Id -ne $originalPid){throw 'Activation duplicated the interface'}
 Write-Output 'Protocol opened GUI; shared tray action and repeated protocol activation reused the same window.'
}finally{
 # Match the exact clone path; never stop the actual user application.
 Get-Process -Name 'NAS Backup Manager' -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq $testApp} | Stop-Process -Force
 $hostFile=Get-Item -LiteralPath (Join-Path (Split-Path $realApp) 'nas-backup-notify.exe')
 NativeAction '--register' $realApp
}
