# Read-only look at why SendInput and CopyFromScreen fail in the RemoteApp session.
param([string]$Out)
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class D {
  [DllImport("user32.dll",SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint f,bool i,uint a);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll",SetLastError=true)] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll",SetLastError=true)] public static extern bool GetCursorPos(out long p);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("kernel32.dll")] public static extern uint WTSGetActiveConsoleSessionId();
}
'@
"PS session id: $((Get-Process -Id $PID).SessionId)  console session: $([D]::WTSGetActiveConsoleSessionId())"
$h = [D]::OpenInputDesktop(0,$false,0x100); "OpenInputDesktop=$h err=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
"Foreground=$([D]::GetForegroundWindow())"
"GetDC(0)=$([D]::GetDC([IntPtr]::Zero))"
"SetCursorPos=$([D]::SetCursorPos(100,100)) err=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
$c=0L; "GetCursorPos=$([D]::GetCursorPos([ref]$c)) value=$c err=$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
"Virtual screen: $([D]::GetSystemMetrics(76)),$([D]::GetSystemMetrics(77)) $([D]::GetSystemMetrics(78))x$([D]::GetSystemMetrics(79))"
"--- query session"; query session 2>&1
"--- logonui / lock"; Get-Process logonui,LockApp -ErrorAction SilentlyContinue | Select Name,Id,SessionId | Format-Table | Out-String
"--- whoami"; whoami
"--- processes in this session of note"; Get-Process rdpinit,rdpshell,explorer,dwm -ErrorAction SilentlyContinue | Select Name,Id,SessionId | Format-Table | Out-String
