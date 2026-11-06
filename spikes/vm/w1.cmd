@echo off
rem Copies the staged self-contained build to C:\mdview-w0, then runs the given PowerShell script from this folder.
rem winapps manual takes no arguments, so one small wrapper .cmd per script calls this: w1.cmd <script.ps1>
set S=\\tsclient\home\markdown_viewer\spikes\winui\stage\self-contained
set V=\\tsclient\home\markdown_viewer\spikes\vm
set O=%V%\w1-out
set D=C:\mdview-w0
if not exist "%O%" mkdir "%O%"
del /q "%O%\%~n1.done" 2>nul
robocopy "%S%" "%D%" /MIR /NFL /NDL /NJH /NJS /NP > "%O%\copy.txt" 2>&1
copy /y "%V%\%~1" "%D%\" >nul
powershell -NoProfile -ExecutionPolicy Bypass -File "%D%\%~1" -Out "%O%" > "%O%\%~n1.txt" 2>&1
echo done> "%O%\%~n1.done"
