# Opt-in measurement of an isolated GUI clone and its WebView descendants.
param([ValidateSet('x64','x86')][string]$Architecture='x64', [ValidateSet('before','after','advanced','release020')][string]$Label='after', [int]$Seconds=8,[switch]$TestBuild)
$ErrorActionPreference='Stop'
if($Seconds -lt 2 -or $Seconds -gt 30){throw 'Use 2..30 seconds'}
$root=Split-Path -Parent $PSScriptRoot
$folder=if($Architecture -eq 'x86'){'NAS Backup Manager x86'}else{'NAS Backup Manager'}
if($TestBuild){$folder+=" Preview"}
$package=Join-Path (Join-Path $root 'portable') $folder
$fixture=Join-Path $root ('target/idle-measure-'+[guid]::NewGuid())
New-Item -ItemType Directory -Path $fixture | Out-Null
foreach($name in @('NAS Backup Manager.exe','nas-backup-notify.exe')){Copy-Item -LiteralPath (Join-Path $package $name) -Destination $fixture}
Set-Content -LiteralPath (Join-Path $fixture 'portable.flag') -Value 'Disposable idle measurement'
$executable=Join-Path $fixture 'NAS Backup Manager.exe'
$app=Start-Process -FilePath $executable -WorkingDirectory $fixture -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds 5
    if($app.HasExited){throw 'Test GUI exited before measurement'}
    $inventory=@(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId)
    $ids=[System.Collections.Generic.HashSet[int]]::new(); [void]$ids.Add($app.Id)
    do {
        $changed=$false
        foreach($entry in $inventory){if($ids.Contains([int]$entry.ParentProcessId) -and $ids.Add([int]$entry.ProcessId)){$changed=$true}}
    } while($changed)
    $processes=@($ids | ForEach-Object {Get-Process -Id $_ -ErrorAction SilentlyContinue})
    $initialCpu=($processes | ForEach-Object {$_.TotalProcessorTime.TotalMilliseconds} | Measure-Object -Sum).Sum
    $clock=[Diagnostics.Stopwatch]::StartNew(); $peakWorking=0L; $peakPrivate=0L
    do {
        $working=0L; $private=0L
        foreach($process in $processes){$process.Refresh();if(-not $process.HasExited){$working+=$process.WorkingSet64;$private+=$process.PrivateMemorySize64}}
        $peakWorking=[Math]::Max($peakWorking,$working);$peakPrivate=[Math]::Max($peakPrivate,$private)
        Start-Sleep -Milliseconds 250
    }while($clock.Elapsed.TotalSeconds -lt $Seconds)
    $elapsed=$clock.Elapsed.TotalSeconds
    $cpu=(($processes | ForEach-Object {$_.Refresh();$_.TotalProcessorTime.TotalMilliseconds} | Measure-Object -Sum).Sum)-$initialCpu
    $machine=Get-CimInstance Win32_ComputerSystem; $processor=Get-CimInstance Win32_Processor | Select-Object -First 1
    $result=[ordered]@{architecture=$Architecture;label=$Label;os=[Environment]::OSVersion.VersionString;os64Bit=[Environment]::Is64BitOperatingSystem;cpu=$processor.Name;logicalCores=[Environment]::ProcessorCount;ramGiB=[Math]::Round($machine.TotalPhysicalMemory/1GB,2);seconds=[Math]::Round($elapsed,3);processes=$processes.Count;cpuMs=[Math]::Round($cpu,3);oneCoreCpuPercent=[Math]::Round($cpu/($elapsed*10),3);peakWorkingSetMiB=[Math]::Round($peakWorking/1MB,2);peakPrivateMiB=[Math]::Round($peakPrivate/1MB,2);scenario='Empty visible GUI plus WebView descendants; no jobs or NAS; 5s startup excluded'}
    $json=$result | ConvertTo-Json
    $json | Set-Content -LiteralPath (Join-Path $root "docs/qa/perf-idle-$Architecture-$Label.json") -Encoding utf8
    $json
}finally {
    # Exact clone only. No user process, configuration or backup is touched.
    $owned=Get-Process -Id $app.Id -ErrorAction SilentlyContinue
    if($owned -and $owned.Path -eq $executable){
        [void]$owned.CloseMainWindow()
        if(-not $owned.WaitForExit(5000)){Stop-Process -Id $owned.Id}
    }
}
