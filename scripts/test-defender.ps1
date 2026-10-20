# Read-only protection verification plus optional native scan of release executables.
# Never changes Defender policy, creates exclusions or restores quarantined files.
param([switch]$Scan, [datetimeoffset]$Since = [datetimeoffset]::UtcNow, [ValidateSet('x64','x86')][string]$Architecture='x64')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$folderName = if($Architecture -eq 'x86'){'NAS Backup Manager x86'}else{'NAS Backup Manager'}
$folder = Join-Path (Join-Path $projectRoot 'portable') $folderName
$status = Get-MpComputerStatus
if (-not $status.AntivirusEnabled -or -not $status.RealTimeProtectionEnabled -or -not $status.BehaviorMonitorEnabled) {
    throw 'Cannot validate the release while antivirus, real-time or behavior protection is disabled.'
}
$names = @('NAS Backup Manager.exe','nas-backup.exe','nas-backup-notify.exe')
if ($Scan) {
    foreach ($name in $names) { Start-MpScan -ScanType CustomScan -ScanPath (Join-Path $folder $name) }
}
$artifacts = foreach ($name in $names) {
    $path = Join-Path $folder $name
    $hash = Get-FileHash -LiteralPath $path -Algorithm SHA256
    [PSCustomObject]@{name=$name;sha256=$hash.Hash;bytes=(Get-Item -LiteralPath $path).Length}
}
# Operational events also catch repeated detections attached to an older threat record.
$events = @(Get-WinEvent -FilterHashtable @{
    LogName='Microsoft-Windows-Windows Defender/Operational';Id=1116;StartTime=$Since.LocalDateTime
} -ErrorAction SilentlyContinue | Where-Object {$_.Message -match [regex]::Escape($projectRoot)})
$detections = @(Get-MpThreatDetection | Where-Object {
    $_.InitialDetectionTime -ge $Since.LocalDateTime -and ($_.Resources -join ' ') -match [regex]::Escape($projectRoot)
})
$report = [PSCustomObject]@{
    sinceUtc=$Since.ToUniversalTime().ToString('o');checkedUtc=[datetimeoffset]::UtcNow.ToString('o')
    os=[Environment]::OSVersion.VersionString;architecture=$Architecture;os64Bit=[Environment]::Is64BitOperatingSystem
    antivirusEnabled=[bool]$status.AntivirusEnabled;realTimeProtectionEnabled=[bool]$status.RealTimeProtectionEnabled
    behaviorMonitorEnabled=[bool]$status.BehaviorMonitorEnabled;signatureVersion=$status.AntivirusSignatureVersion
    observedProjectDetectionEvents=$events.Count;observedProjectDetections=$detections.Count;artifacts=@($artifacts)
}
$reportName=if($Architecture -eq 'x86'){'defender-verification-x86.json'}else{'defender-verification.json'}
$destination = Join-Path (Join-Path $projectRoot 'docs/qa') $reportName
$report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $destination -Encoding utf8
if ($events.Count -or $detections.Count) { throw "Defender detected this project during verification. See $destination" }
Write-Output "Protection stayed enabled; $($artifacts.Count) release files readable; no project detections since $($Since.ToString('o'))."
