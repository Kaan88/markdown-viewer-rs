# Proves defect 3 is fixed: a forced OnLaunched failure shows a message box, then exits with code 1.
param([string]$Out)
$D = 'C:\mdview-w0'
Add-Type -AssemblyName UIAutomationClient; Add-Type -AssemblyName UIAutomationTypes
$p = Start-Process "$D\mdview-winui.exe" -ArgumentList '--test-fail-launch','--trace' -PassThru
$deadline = (Get-Date).AddSeconds(15)
while ($p.MainWindowHandle -eq 0 -and -not $p.HasExited -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200; $p.Refresh() }
"window title=[$($p.MainWindowTitle)] exited=$($p.HasExited)"
Start-Sleep -Seconds 2
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Drawing;
public static class B { [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [StructLayout(LayoutKind.Sequential)] public struct R { public int L,T,Rt,B2; }
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
 public static void Shot(IntPtr h,string f){ R r; GetWindowRect(h,out r); var b=new Bitmap(r.Rt-r.L,r.B2-r.T); using(var g=Graphics.FromImage(b)){ g.CopyFromScreen(r.L,r.T,0,0,b.Size);} b.Save(f);} }
"@ -ReferencedAssemblies System.Drawing
[B]::Shot($p.MainWindowHandle, "$Out\shot-failbox.png")
"box shot saved; sending WM_CLOSE"
$null = [B]::PostMessageW($p.MainWindowHandle, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
if ($p.WaitForExit(5000)) { "exit code after dismiss=$($p.ExitCode)" } else { "STILL RUNNING after dismiss"; $p.Kill() }
Remove-Item "$D\once.log" -ErrorAction SilentlyContinue
$q = Start-Process "$D\mdview-winui.exe" -ArgumentList '--test-fail-launch','--measure=C:\mdview-w0\once.log' -PassThru
if ($q.WaitForExit(15000)) { "measure-mode exit code=$($q.ExitCode)" } else { "measure-mode HUNG"; $q.Kill() }
Get-Content "$D\once.log"
