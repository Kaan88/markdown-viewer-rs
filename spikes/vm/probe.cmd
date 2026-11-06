@echo off
set O=\\tsclient\home\markdown_viewer\spikes\vm\probe.txt
ver > %O%
whoami >> %O%
wmic os get Caption,BuildNumber /value >> %O% 2>&1
powershell -NoProfile -Command "Get-AppxPackage -AllUsers *WindowsAppRuntime* | Select Name,Version | Format-Table -Auto | Out-String; (Get-CimInstance Win32_VideoController).Name; Get-ComputerInfo -Property OsBuildNumber,WindowsProductName | Out-String" >> %O% 2>&1
