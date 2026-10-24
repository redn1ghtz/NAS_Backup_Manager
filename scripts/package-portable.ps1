param([switch]$SkipBuild, [ValidateSet('x64','x86')][string]$Architecture='x64', [switch]$TestBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$targetArguments = @()
$artifactDirectory = Join-Path $projectRoot 'target/release'
$folderName = 'NAS Backup Manager'
if ($Architecture -eq 'x86') {
    $targetArguments = @('--target','i686-pc-windows-msvc')
    $artifactDirectory = Join-Path $projectRoot 'target/i686-pc-windows-msvc/release'
    $folderName = 'NAS Backup Manager x86'
}
if (-not $SkipBuild) {
    Push-Location (Join-Path $projectRoot 'apps/desktop')
    try { & npm.cmd run tauri -- build --ci --no-bundle @targetArguments; if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' } }
    finally { Pop-Location }
    Push-Location $projectRoot
    try { & cargo build --release --locked -p nas-backup-cli @targetArguments; if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' } }
    finally { Pop-Location }
}
if($TestBuild){$folderName += " Preview"}
$outputDirectory = Join-Path (Join-Path $projectRoot 'portable') $folderName
$hostArtifact = Join-Path $artifactDirectory 'nas-backup-notify.exe'
if (-not (Test-Path -LiteralPath $hostArtifact -PathType Leaf)) { throw 'Notification sidecar is missing; build the Windows storage crate first' }
$expectedVersion=(Get-Content -LiteralPath (Join-Path $projectRoot 'apps/desktop/package.json') -Raw | ConvertFrom-Json).version
$artifactVersion=[Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path $artifactDirectory 'nas-backup-desktop.exe')).ProductVersion
if($artifactVersion -ne $expectedVersion){throw "Rebuild desktop first: expected $expectedVersion, found $artifactVersion"}
# Reject a mixed-architecture package before replacing any existing files.
$expectedMachine = if($Architecture -eq 'x86'){0x014c}else{0x8664}
foreach($name in @('nas-backup-desktop.exe','nas-backup.exe','nas-backup-notify.exe')) {
    $stream=[System.IO.File]::OpenRead((Join-Path $artifactDirectory $name))
    try {
        $reader=[System.IO.BinaryReader]::new($stream)
        $stream.Position=0x3c; $header=$reader.ReadInt32(); $stream.Position=$header
        if($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne $expectedMachine){throw "Wrong PE architecture: $name"}
    } finally { $stream.Dispose() }
}
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
Copy-Item -LiteralPath $hostArtifact -Destination (Join-Path $outputDirectory 'nas-backup-notify.exe') -Force
Copy-Item -LiteralPath (Join-Path $artifactDirectory 'nas-backup-desktop.exe') -Destination (Join-Path $outputDirectory 'NAS Backup Manager.exe') -Force
Copy-Item -LiteralPath (Join-Path $artifactDirectory 'nas-backup.exe') -Destination $outputDirectory -Force
Set-Content -LiteralPath (Join-Path $outputDirectory 'portable.flag') -Value 'Local application state in data/' -Encoding utf8
Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $outputDirectory -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD_PARTY_NOTICES.md') -Destination $outputDirectory -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/USER_GUIDE.md') -Destination $outputDirectory -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/VALIDATION.md') -Destination $outputDirectory -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/PORTABLE.md') -Destination (Join-Path $outputDirectory 'README.md') -Force
$licenseSnapshot=Join-Path $projectRoot "target/third-party-licenses/$expectedVersion"
if(Test-Path -LiteralPath $licenseSnapshot -PathType Container) {
    $licenseDirectory=Join-Path $outputDirectory 'licenses'
    New-Item -ItemType Directory -Path $licenseDirectory -Force | Out-Null
    Copy-Item -Path (Join-Path $licenseSnapshot '*') -Destination $licenseDirectory -Recurse -Force
}
Write-Output $outputDirectory

$optionalTools=Join-Path $projectRoot "target/optional-tools/$Architecture"
if(Test-Path -LiteralPath $optionalTools -PathType Container){
 $toolDestination=Join-Path $outputDirectory 'tools'
 New-Item -ItemType Directory -Path $toolDestination -Force | Out-Null
 foreach($name in @('age.exe','age-keygen.exe','rclone.exe','LICENSE','AGE-LICENSE','RCLONE-COPYING','BUILD-SOURCE.json')){
  $file=Join-Path $optionalTools $name
  if(Test-Path -LiteralPath $file -PathType Leaf){Copy-Item -LiteralPath $file -Destination $toolDestination -Force}
 }
 if(Test-Path -LiteralPath (Join-Path $optionalTools 'notices')){Copy-Item -LiteralPath (Join-Path $optionalTools 'notices') -Destination $toolDestination -Recurse -Force}
}
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/ADVANCED.md') -Destination $outputDirectory -Force
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/TOOL_SOURCES.md') -Destination $outputDirectory -Force
