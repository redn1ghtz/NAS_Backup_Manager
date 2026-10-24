# Explicit optional tool download; never modifies system PATH or installs a service.
param([ValidateSet('x64','x86')][string]$Architecture='x64',[switch]$BuildAgeX86)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$cache=Join-Path $root 'target/optional-tools'
$archFolder=Join-Path $cache $Architecture
New-Item -ItemType Directory -Path $archFolder -Force | Out-Null
$arch=if($Architecture -eq 'x64'){'amd64'}else{'386'}
$assets=@(@{Name="rclone-v1.75.2-windows-$arch.zip";Url="https://github.com/rclone/rclone/releases/download/v1.75.2/rclone-v1.75.2-windows-$arch.zip";Hash=if($Architecture -eq 'x64'){'8a9db2bad1dc79a65b3348fd857069598e2aab7cf4d118b77b1df23055aff818'}else{'904876ca77f99f29400dfbb41ff99c132f64f1cdfa50fe7dc7923683b252186d'}})
if($Architecture -eq 'x64'){$assets+=@{Name='age-v1.3.2-windows-amd64.zip';Url='https://github.com/FiloSottile/age/releases/download/v1.3.2/age-v1.3.2-windows-amd64.zip';Hash='f48d8f8f9ebe903ab5027ed067652f2cc1db94bc206976430133b905dcd8e8c7'}}
foreach($asset in $assets){
 $archive=Join-Path $cache $asset.Name
 if(-not(Test-Path -LiteralPath $archive)){Invoke-WebRequest -Uri $asset.Url -OutFile $archive}
 if((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $asset.Hash){throw "Hash mismatch: $($asset.Name)"}
 $expanded=Join-Path $cache ([IO.Path]::GetFileNameWithoutExtension($asset.Name))
 if(-not(Test-Path -LiteralPath $expanded)){Expand-Archive -LiteralPath $archive -DestinationPath $expanded}
 foreach($name in @('age.exe','age-keygen.exe','rclone.exe','COPYING','LICENSE','LICENSE.txt','README.txt')){
  Get-ChildItem -LiteralPath $expanded -File -Recurse | Where-Object Name -eq $name | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $archFolder $_.Name) -Force}
 }
}
if($Architecture -eq 'x86' -and -not(Test-Path -LiteralPath (Join-Path $archFolder 'age.exe'))){Write-Warning 'Upstream age has no official x86 binary. Build age for GOARCH=386 before testing encryption on native 32-bit Windows.'}
Write-Output $archFolder

Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/rclone/rclone/v1.75.2/COPYING' -OutFile (Join-Path $archFolder 'RCLONE-COPYING')
$ageLicense=Join-Path $cache 'x64/LICENSE'
if(Test-Path -LiteralPath $ageLicense){Copy-Item -LiteralPath $ageLicense -Destination (Join-Path $archFolder 'AGE-LICENSE') -Force}
if($Architecture -eq 'x86' -and $BuildAgeX86){
 $goArchive=Join-Path $cache 'go1.27.3.windows-amd64.zip'
 if(-not(Test-Path -LiteralPath $goArchive)){Invoke-WebRequest -Uri 'https://go.dev/dl/go1.27.3.windows-amd64.zip' -OutFile $goArchive}
 if((Get-FileHash -LiteralPath $goArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'f26081f438319757f8a1b46d7d0b2c5ec7583d4038e485e0aa725cfde7885fff'){throw 'Go checksum mismatch'}
 $goFolder=Join-Path $cache 'go-toolchain'
 if(-not(Test-Path -LiteralPath $goFolder)){Expand-Archive -LiteralPath $goArchive -DestinationPath $goFolder}
 $env:GOOS='windows';$env:GOARCH='386';$env:CGO_ENABLED='0';$env:GOTOOLCHAIN='local';$env:GOPATH=Join-Path $cache 'go-work';$env:GOCACHE=Join-Path $cache 'go-cache'
 foreach($name in @('age','age-keygen')){& (Join-Path $goFolder 'go/bin/go.exe') install "filippo.io/age/cmd/$name@v1.3.2";if($LASTEXITCODE -ne 0){throw 'age x86 build failed'};Copy-Item -LiteralPath (Join-Path $env:GOPATH "bin/windows_386/$name.exe") -Destination $archFolder -Force}
}
