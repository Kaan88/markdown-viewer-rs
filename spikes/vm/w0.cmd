@echo off
set S=\\tsclient\home\markdown_viewer\spikes\winui\stage\self-contained
set O=\\tsclient\home\markdown_viewer\spikes\vm\w0-out
set D=C:\mdview-w0
robocopy "%S%" "%D%" /MIR /NFL /NDL /NJH /NJS /NP > "%O%\copy.txt" 2>&1
copy /y \\tsclient\home\markdown_viewer\spikes\winui\measure.ps1 "%D%\" >nul
cd /d "%D%"
del mdview-winui.trace.log 2>nul
start "" /wait powershell -NoProfile -Command "$p=Start-Process .\mdview-winui.exe -ArgumentList '--trace','--measure=C:\mdview-w0\once.log' -PassThru; if(-not $p.WaitForExit(60000)){$p.Kill(); 'HUNG'}else{'exit='+$p.ExitCode}" > "%O%\run.txt" 2>&1
copy /y mdview-winui.trace.log "%O%\" >nul 2>&1
copy /y once.log "%O%\" >nul 2>&1
powershell -NoProfile -Command "Get-WinEvent -FilterHashtable @{LogName='Application';Level=2;StartTime=(Get-Date).AddMinutes(-5)} -MaxEvents 5 | Format-List TimeCreated,ProviderName,Message | Out-String -Width 200" > "%O%\events.txt" 2>&1
echo done> "%O%\done.txt"
