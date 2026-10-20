param([ValidateSet('x64','x86','both')][string]$Architecture='both',[string]$ISCC)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
if(-not $ISCC) {
    $candidates=@('C:/Program Files/Inno Setup 7/ISCC.exe','C:/Program Files (x86)/Inno Setup 7/ISCC.exe')
    $ISCC=$candidates | Where-Object {Test-Path -LiteralPath $_} | Select-Object -First 1
}
if(-not $ISCC){throw 'Install Inno Setup 7, or pass -ISCC with its absolute ISCC.exe path'}
$targets=if($Architecture -eq 'both'){@('x64','x86')}else{@($Architecture)}
foreach($target in $targets) {
    & $ISCC (Join-Path $root "releases/0.1.0-alpha/setup-$target.iss")
    if($LASTEXITCODE -ne 0){throw "Inno compilation failed: $target"}
}
