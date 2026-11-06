# Runs mdview-winui.exe --measure N times and prints first-frame and memory figures.
# Usage, from the folder holding the exe:  powershell -ExecutionPolicy Bypass -File .\measure.ps1 [-Runs 10]
# The first run is cold. It is logged but excluded from the warm summary.
param([int]$Runs = 10)

$exe = Join-Path $PSScriptRoot 'mdview-winui.exe'
$log = Join-Path $PSScriptRoot 'measure.log'
$trace = Join-Path $PSScriptRoot 'mdview-winui.trace.log'
if (-not (Test-Path $exe)) { throw "mdview-winui.exe not found beside this script" }
if (Test-Path $log) { Remove-Item $log }

for ($i = 1; $i -le $Runs; $i++) {
    # Quoted, because the folder may contain spaces.
    $p = Start-Process -FilePath $exe -ArgumentList "`"--measure=$log`"" -PassThru
    if (-not $p.WaitForExit(60000)) { $p.Kill(); Write-Warning "run $i hung and was killed" }
    elseif ($p.ExitCode -ne 0) { Write-Warning "run $i exited with code $($p.ExitCode)" }
    # Each launch replaces the trace, so keep every run's copy for diagnosing a failure.
    if (Test-Path $trace) { Copy-Item $trace (Join-Path $PSScriptRoot "trace-run$i.log") }
    Start-Sleep -Milliseconds 500
}

if (-not (Test-Path $log)) { Write-Warning "no run wrote measure.log; see trace-run1.log beside this script"; exit 1 }

$rows = @(Get-Content $log | ForEach-Object {
    $row = @{}
    foreach ($field in $_ -split ' ') { $k, $v = $field -split '=', 2; $row[$k] = $v }
    [pscustomobject]$row
})

Get-Content $log
$ok = @($rows | Where-Object { $_.result -eq 'ok' })
"`nruns: $Runs  ok: $($ok.Count)  mode: $($rows[0].mode)"
if ($ok.Count -lt 2) { Write-Warning "too few successful runs to summarise; see trace-run*.log"; exit 1 }

function Summary($name, $values, $scale, $unit) {
    $sorted = @($values | ForEach-Object { [double]$_ / $scale } | Sort-Object)
    $median = $sorted[[int][math]::Floor($sorted.Count / 2)]
    '{0,-26} min {1,8:N1}  median {2,8:N1}  max {3,8:N1} {4}' -f $name, $sorted[0], $median, $sorted[-1], $unit
}

$warm = @($ok | Select-Object -Skip 1)
Summary 'first frame (warm)' $warm.first_frame_ms 1 'ms'
Summary 'full UI (warm)' $warm.full_ui_ms 1 'ms'
Summary 'private, first frame' $warm.private_bytes_first_frame 1048576 'MiB'
Summary 'private, full UI' $warm.private_bytes_full_ui 1048576 'MiB'
Summary 'working set, full UI' $warm.working_set_full_ui 1048576 'MiB'
"first frame (cold, run 1)  $($ok[0].first_frame_ms) ms"
