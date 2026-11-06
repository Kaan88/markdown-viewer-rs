# Reports a mdview-winui.exe left by see-it.sh's test, then ends it.
param([string]$Out)
$a = @(Get-Process mdview-winui -ErrorAction SilentlyContinue)
"running: $($a.Count)"
foreach ($p in $a) { "pid $($p.Id) session $($p.SessionId) title=[$($p.MainWindowTitle)] handle=$($p.MainWindowHandle) ws=$($p.WorkingSet64) private=$($p.PrivateMemorySize64) started=$($p.StartTime)"; $p.Kill() }
