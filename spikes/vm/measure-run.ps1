# Runs measure.ps1 for 10 runs, then repeats 3 runs from a folder with a space, using the exact W0.md command form.
param([string]$Out)
$D = 'C:\mdview-w0'
Set-Location $D
"--- $D, 10 runs, in process"
& "$D\measure.ps1" -Runs 12
Copy-Item "$D\measure.log" "$Out\measure.log"
Copy-Item "$D\trace-run12.log" "$Out\measure-trace-run12.log"
$S = "$D\with space"
$null = robocopy $D $S /E /XD $S /XF *.log /NFL /NDL /NJH /NJS /NP
"--- $S, 3 runs, W0.md command form in a child powershell"
Set-Location $S
$t=[Diagnostics.Stopwatch]::StartNew()
$c = Start-Process powershell -ArgumentList '-ExecutionPolicy','Bypass','-File','.\measure.ps1','-Runs','3' -RedirectStandardOutput "$D\space-out.txt" -RedirectStandardError "$D\space-err.txt" -PassThru
if ($c.WaitForExit(60000)) { "child exit=$($c.ExitCode) after $($t.ElapsedMilliseconds) ms" } else { "child HUNG after 60 s"; $c.Kill() }
Get-Content "$D\space-out.txt"; Get-Content "$D\space-err.txt"
Set-Location $D
Remove-Item $S -Recurse -Force
"--- leftovers"; Get-Process mdview-winui -ErrorAction SilentlyContinue | Select Id,SessionId
