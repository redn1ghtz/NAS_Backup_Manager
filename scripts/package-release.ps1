param([switch]$SkipBuild)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$version=(Get-Content -LiteralPath (Join-Path $root 'apps/desktop/package.json') -Raw | ConvertFrom-Json).version
if($version -ne '0.1.0-alpha'){throw 'Update the Inno version definitions before changing release version'}
$output=Join-Path $root "releases/$version"
foreach($architecture in @('x64','x86')) {
    & (Join-Path $PSScriptRoot 'package-portable.ps1') -Architecture $architecture -SkipBuild:$SkipBuild
    $folder=if($architecture -eq 'x64'){'NAS Backup Manager'}else{'NAS Backup Manager x86'}
    $source=Join-Path (Join-Path $root 'portable') $folder
    $destination=Join-Path $output "portable-$architecture"
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    foreach($file in @('NAS Backup Manager.exe','nas-backup.exe','nas-backup-notify.exe','portable.flag','LICENSE','THIRD_PARTY_NOTICES.md','README.md','USER_GUIDE.md','VALIDATION.md')) {
        Copy-Item -LiteralPath (Join-Path $source $file) -Destination $destination -Force
    }
    Copy-Item -LiteralPath (Join-Path $root 'docs/INSTALLER.md') -Destination $destination -Force
    if(Test-Path -LiteralPath (Join-Path $source 'licenses') -PathType Container) {
        $licenses=Join-Path $destination 'licenses'
        New-Item -ItemType Directory -Path $licenses -Force | Out-Null
        Copy-Item -Path (Join-Path $source 'licenses/*') -Destination $licenses -Recurse -Force
    }
    $wrapper=Get-Content -LiteralPath (Join-Path $root "installers/inno/setup-$architecture.iss") -Raw
    ('#define ReleaseRoot SourcePath' + "`r`n" + $wrapper) | Set-Content -LiteralPath (Join-Path $output "setup-$architecture.iss") -Encoding utf8
}
Copy-Item -LiteralPath (Join-Path $root 'installers/inno/setup-common.iss') -Destination $output -Force
Copy-Item -LiteralPath (Join-Path $root 'docs/INSTALLER.md') -Destination (Join-Path $output 'BUILD_INSTALLERS.md') -Force
Write-Output "Release folder: $output"
