param([string]$Out)
$D='C:\mdview-w0'
Set-Location $D
foreach ($i in 1..3) {
  $t=[Diagnostics.Stopwatch]::StartNew()
  $p = Start-Process "$D\mdview-winui.exe" -ArgumentList "`"--measure=$D\m.log`"" -PassThru
  $r = $p.WaitForExit(30000)
  "run $i waited=$r ms=$($t.ElapsedMilliseconds) exit=$($p.ExitCode)"
  if (-not $r) { $p.Kill() }
  Get-Content "$D\mdview-winui.trace.log" | Select-Object -Last 4
  Start-Sleep -Milliseconds 500
}
"--- m.log"; Get-Content "$D\m.log"
"--- file measure.ps1"
$t=[Diagnostics.Stopwatch]::StartNew()
& "$D\measure.ps1" -Runs 3
"direct: $($t.ElapsedMilliseconds) ms"
