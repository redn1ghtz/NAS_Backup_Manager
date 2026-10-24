param([switch]$SkipBuild)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$version=(Get-Content -LiteralPath (Join-Path $root 'apps/desktop/package.json') -Raw | ConvertFrom-Json).version
$inno=Get-Content -LiteralPath (Join-Path $root 'installers/inno/setup-common.iss') -Raw
if($inno -notmatch ('#define AppVersion "'+[regex]::Escape($version)+'"')){throw 'Inno and application versions differ'}
$output=Join-Path $root "releases/$version"
& python (Join-Path $PSScriptRoot 'prepare-github.py') --licenses-only
if($LASTEXITCODE -ne 0){throw 'Dependency notice collection failed'}
foreach($architecture in @('x64','x86')) {
    if(-not(Test-Path -LiteralPath (Join-Path $root "target/optional-tools/$architecture/notices/modules/inventory.json"))){throw 'Run scripts/collect-tool-notices.py before packaging'}
    & (Join-Path $PSScriptRoot 'package-portable.ps1') -Architecture $architecture -TestBuild -SkipBuild:$SkipBuild
    $folder=if($architecture -eq 'x64'){'NAS Backup Manager Preview'}else{'NAS Backup Manager x86 Preview'}
    $source=Join-Path (Join-Path $root 'portable') $folder
    $destination=Join-Path $output "portable-$architecture"
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    foreach($file in @('NAS Backup Manager.exe','nas-backup.exe','nas-backup-notify.exe','portable.flag','LICENSE','THIRD_PARTY_NOTICES.md','README.md','USER_GUIDE.md','VALIDATION.md','ADVANCED.md','TOOL_SOURCES.md')) {
        Copy-Item -LiteralPath (Join-Path $source $file) -Destination $destination -Force
    }
    $tools=Join-Path $destination 'tools'
    New-Item -ItemType Directory -Path $tools -Force | Out-Null
    foreach($name in @('age.exe','age-keygen.exe','rclone.exe','AGE-LICENSE','RCLONE-COPYING')) {
        Copy-Item -LiteralPath (Join-Path $source "tools/$name") -Destination $tools -Force
    }
    Copy-Item -LiteralPath (Join-Path $source 'tools/notices') -Destination $tools -Recurse -Force
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
