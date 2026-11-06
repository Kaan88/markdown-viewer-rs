# Launches the app once, screenshots it inside the VM, drives wheel, tree, flyout, typing and File > Close,
# and writes everything to -Out. Windows-side test harness only; no part of the app.
param([string]$Out)
$ErrorActionPreference = 'Continue'
$D = 'C:\mdview-w0'
Set-Location $D
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Drawing;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L,T,Rt,B; }
  [StructLayout(LayoutKind.Sequential)] public struct MI { public int dx,dy; public uint data,flags,time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] public struct KI { public ushort vk,scan; public uint flags,time; public IntPtr extra; public int pad1,pad2; }
  [StructLayout(LayoutKind.Explicit)] public struct U { [FieldOffset(0)] public MI mi; [FieldOffset(0)] public KI ki; }
  [StructLayout(LayoutKind.Sequential)] public struct IN { public uint type; public U u; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, IN[] i, int cb);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetDpiForSystem();
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, System.Text.StringBuilder s, int n);
  public static string Title(IntPtr h){ var s=new System.Text.StringBuilder(256); GetWindowTextW(h,s,256); return s.ToString(); }
  public static uint Sent=0, Failed=0; static void Send(IN x){ uint n=SendInput(1,new[]{x},Marshal.SizeOf(typeof(IN))); if(n==1) Sent++; else Failed++; }
  [DllImport("user32.dll",SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint f,bool i,uint a);
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h);
  static IN M(uint flags,uint data){ var x=new IN(); x.type=0; x.u.mi.flags=flags; x.u.mi.data=data; return x; }
  public static void Move(int x,int y){ SetCursorPos(x,y); System.Threading.Thread.Sleep(150); Send(M(1,0)); }
  public static void Wheel(int d){ Send(M(0x800,(uint)d)); }
  public static void Left(){ Send(M(2,0)); System.Threading.Thread.Sleep(70); Send(M(4,0)); }
  public static void Right(){ Send(M(8,0)); System.Threading.Thread.Sleep(70); Send(M(0x10,0)); }
  public static void Key(char c){ var x=new IN(); x.type=1; x.u.ki.scan=c; x.u.ki.flags=4; Send(x); x.u.ki.flags=6; Send(x); }
  public static Bitmap Print(IntPtr h,uint f){ R r; GetWindowRect(h,out r); var b=new Bitmap(r.Rt-r.L,r.B-r.T); using(var g=Graphics.FromImage(b)){ var dc=g.GetHdc(); PrintWindow(h,dc,f); g.ReleaseHdc(dc);} return b; }
  public static Bitmap Screen(IntPtr h){ R r; GetWindowRect(h,out r); var b=new Bitmap(r.Rt-r.L,r.B-r.T); using(var g=Graphics.FromImage(b)){ g.CopyFromScreen(r.L,r.T,0,0,b.Size);} return b; }
}
'@

function Shot($name) { try { $b = [W]::Screen($h); $b.Save("$Out\$name.png"); $b.Dispose(); Log "$name saved" } catch { Log "$name FAILED: $($_.Exception.Message)" } }
function Log($m) { "{0:HH:mm:ss.fff} {1}" -f (Get-Date), $m }
function TraceHas($t) { (Get-Content "$D\mdview-winui.trace.log" -ErrorAction SilentlyContinue) -match $t }
$AE = [Windows.Automation.AutomationElement]
function ByName($root, $name) {
    $root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        (New-Object Windows.Automation.PropertyCondition($AE::NameProperty, $name)))
}
function Under($x, $y) { try { $e = $AE::FromPoint((New-Object Windows.Point($x, $y))); Log "under $x,$y = $($e.Current.ControlType.ProgrammaticName) [$($e.Current.Name)] class=[$($e.Current.ClassName)]" } catch { Log "under failed $_" } }
function Center($e) { $r = $e.Current.BoundingRectangle; @([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2)) }

Remove-Item "$D\mdview-winui.trace.log" -ErrorAction SilentlyContinue
$p = Start-Process "$D\mdview-winui.exe" -ArgumentList '--trace' -PassThru
Log "started pid $($p.Id)"
$deadline = (Get-Date).AddSeconds(30)
while ($p.MainWindowHandle -eq 0 -and -not $p.HasExited -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200; $p.Refresh() }
if ($p.HasExited) { Log "EXITED early code=$($p.ExitCode)" }
elseif ($p.MainWindowHandle -eq 0) { Log "NO WINDOW after 30 s" }
else {
    $h = $p.MainWindowHandle
    Start-Sleep -Seconds 4
    $null = [W]::SetForegroundWindow($h)
    Start-Sleep -Milliseconds 800
    Log "inputDesktop=$([W]::OpenInputDesktop(0,$false,0x100)) GetDC0=$([W]::GetDC([IntPtr]::Zero)) fg=$([W]::GetForegroundWindow()) fgTitle=[$([W]::Title([W]::GetForegroundWindow()))] h=$h"
    Log "title=[$([W]::Title($h))] foreground=$([W]::GetForegroundWindow() -eq $h) dpiWindow=$([W]::GetDpiForWindow($h)) dpiSystem=$([W]::GetDpiForSystem())"
    $r = New-Object W+R; $null = [W]::GetWindowRect($h, [ref]$r); Log "rect L=$($r.L) T=$($r.T) R=$($r.Rt) B=$($r.B)"
    Shot shot-screen
    $bmp = [W]::Print($h, 2); $bmp.Save("$Out\shot-print.png"); $bmp.Dispose()
    Log "screenshots saved"

    $win = $AE::FromHandle($h)
    # UIA name dump, so a miss below can be diagnosed.
    $all = $win.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    foreach ($e in $all) { $b = $e.Current.BoundingRectangle; "UIA {0} name=[{1}] class=[{2}] {3},{4} {5}x{6}" -f $e.Current.ControlType.ProgrammaticName, $e.Current.Name, $e.Current.ClassName, $b.X, $b.Y, $b.Width, $b.Height }

    # Wheel over the panel centre: right of the 240 px tree, mid height.
    $wx = [int](($r.L + 240 + $r.Rt) / 2); $wy = [int](($r.T + $r.B) / 2)
    Log "wheel at $wx,$wy"; Under $wx $wy
    [W]::Move($wx, $wy); Start-Sleep -Milliseconds 200
    [W]::Wheel(-120); Start-Sleep -Milliseconds 300; [W]::Wheel(-120); Start-Sleep -Milliseconds 300; [W]::Wheel(120); Start-Sleep -Milliseconds 500
    Log "wheel lines in trace: $(@(TraceHas 'wheel delta').Count)"
    (TraceHas 'wheel delta') | ForEach-Object { "  $_" }
    Shot shot-wheel

    # Lazy tree: click the expander chevron via the node, or expand through UIA.
    $node = ByName $win 'fixtures (lazy)'
    if ($node) {
        $pat = $null
        if ($node.TryGetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pat)) { $pat.Expand(); Log "tree expanded through UIA" }
        Start-Sleep -Milliseconds 800
        Log "lazy lines in trace: $(@(TraceHas 'lazy tree').Count); child showcase.md present: $([bool](ByName $win 'showcase.md'))"

        # ContextFlyout: right-click node with SendInput, then click the item.
        $c = Center $node
        Log "node centre $($c[0]),$($c[1]) fg=$([W]::Title([W]::GetForegroundWindow()))"; Under $c[0] $c[1]
        Shot shot-before-right
        [W]::Move($c[0], $c[1]); Start-Sleep -Milliseconds 400
        Shot shot-hover
        [W]::Right(); Start-Sleep -Milliseconds 1200
        Shot shot-flyout
        $root = $AE::RootElement
        $item = ByName $win 'Open in new tab'
        if (-not $item) { $item = ByName $root 'Open in new tab' }
        Log "flyout item found through UIA: $([bool]$item)"
        if ($item) {
            $c = Center $item
            [W]::Move($c[0], $c[1]); Start-Sleep -Milliseconds 200; [W]::Left(); Start-Sleep -Milliseconds 800
            Log "flyout clicked: trace lines $(@(TraceHas 'context flyout').Count); title=[$([W]::Title($h))]"
        }
    } else { Log "tree node not found through UIA" }

    # TextBox typing.
    $find = $win.FindFirst([Windows.Automation.TreeScope]::Descendants,
        (New-Object Windows.Automation.PropertyCondition($AE::ControlTypeProperty, [Windows.Automation.ControlType]::Edit)))
    if ($find) {
        $c = Center $find
        Log "find centre $($c[0]),$($c[1])"; Under $c[0] $c[1]
        [W]::Move($c[0], $c[1]); Start-Sleep -Milliseconds 200; [W]::Left(); Start-Sleep -Milliseconds 400
        Log "textbox focused after click: $($find.Current.HasKeyboardFocus)"
        if (-not $find.Current.HasKeyboardFocus) { $find.SetFocus(); Start-Sleep -Milliseconds 300; Log "after UIA SetFocus: $($find.Current.HasKeyboardFocus)" }
        foreach ($ch in 'hello w0'.ToCharArray()) { [W]::Key($ch); Start-Sleep -Milliseconds 40 }
        Start-Sleep -Milliseconds 500
        $v = $null
        if ($find.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$v)) { Log "textbox value=[$($v.Current.Value)]" }
    } else { Log "textbox not found through UIA" }

    Shot shot-final
    $bmp = [W]::Print($h, 2); $bmp.Save("$Out\shot-final-print.png"); $bmp.Dispose()

    # File > Close should end the process.
    $file = ByName $win 'File'
    if ($file) {
        $c = Center $file; Log "file centre $($c[0]),$($c[1])"; Under $c[0] $c[1]
        [W]::Move($c[0], $c[1]); Start-Sleep -Milliseconds 300; [W]::Left(); Start-Sleep -Milliseconds 1000
        Shot shot-menu
        $close = ByName $win 'Close'
        if (-not $close) { $close = ByName $AE::RootElement 'Close' }
        Log "menu Close found: $([bool]$close)"
        if ($close) { Log "close class=[$($close.Current.ClassName)] type=$($close.Current.ControlType.ProgrammaticName)"; $c = Center $close; [W]::Move($c[0], $c[1]); Start-Sleep -Milliseconds 300; [W]::Left() }
    } else { Log "File menu not found through UIA" }
    if ($p.WaitForExit(6000)) { Log "exited after File > Close, code=$($p.ExitCode)" }
}
if (-not $p.HasExited) { Log "still running: killing"; $p.Kill() }
Copy-Item "$D\mdview-winui.trace.log" "$Out\smoke.trace.log" -ErrorAction SilentlyContinue
Get-WinEvent -FilterHashtable @{LogName='Application';Level=2;StartTime=(Get-Date).AddMinutes(-3)} -MaxEvents 5 -ErrorAction SilentlyContinue |
    Format-List TimeCreated,ProviderName,Message | Out-String -Width 200
Log "SendInput ok=$([W]::Sent) failed=$([W]::Failed)"
Log "harness end"
