# G8: WinUI 3 skeleton

**Plan criterion:** "Exe links, and runs on the user's PC."

| Part | Status |
|---|---|
| Exe cross-links, both flavours, staged | **PASS** |
| Self-contained build runs on real Windows 11 (VM, build 26200), with TabView, MenuBar, TreeView, TextBox and SwapChainPanel checkerboard | **PASS** (VM) |
| Wheel, lazy tree, ContextFlyout, typing, File > Close, driven by SendInput and UI Automation | **PASS** (VM), final build 3 of 3 runs; see "Input" for earlier misses |
| 1 px checkerboard at the VM's scale (100%) | **PASS**, checked from pixels |
| 1 px checkerboard at 150, 175, 225% and on live scale change | **NEEDS_USER**. Changing VM display settings isn't allowed here. |
| Framework-dependent build at runtime | **NEEDS_USER**. It needs runtime 2.6, and the VM has only 1.5-1.8. Built, never run. |
| Real launch and memory figures on real hardware with a GPU | **NEEDS_USER** (W0, see `W0.md`). The numbers below are VM proxies. |
| Runs under Wine 11.20 | **FAIL**, as the plan expected. A plain Rust exe does run. |

This targets Windows 11 only, as requested, and there is no accessibility work.

## Measured: Windows 11 VM (proxies, not real launch figures)

**VM:** WinApps Docker, 8 cores, 8 GB, no real GPU, driven over RDP as a RemoteApp, at 100% scale, 96 DPI. The window is 2580 x 1026 and the panel 2324 x 875 physical pixels. Rendering is software. Other work was running on the host (load average 3.8-7.0 on 28 threads), so these are **noisy**.

`measure.ps1 -Runs 10` on the final build, 10 of 10 `result=ok`. Raw lines are in `spikes/vm/w1-out/measure.log`. Warm is runs 2-10.

| Figure | Min | Median | Max |
|---|---|---|---|
| First frame | 110.6 ms | 126.2 ms | 158.3 ms |
| Full UI | 167.8 ms | 171.9 ms | 219.2 ms |
| Private bytes, first frame | 25.3 MiB | 25.7 MiB | 28.9 MiB |
| Private bytes, full UI | 27.8 MiB | 30.2 MiB | 30.4 MiB |
| Working set, full UI | 104.9 MiB | 116.7 MiB | 117.0 MiB |

**Slower starts, all measured once each, not a truly cold boot (no VM reboot was done):**

| Case | First frame |
|---|---|
| Run 1 of the set above, files already in the VM's cache | 146.4 ms |
| First run from a freshly copied folder (`with space`) | 466.2 ms (full UI 549.3 ms) |
| First run after a new exe was copied in, first working build | 314.0 ms |

**What these do and don't say:**
- All figures: measured, as VM proxies. Real hardware will differ in both directions: a real GPU driver costs load time that Basic Render doesn't, and RDP adds its own.
- The first-frame marker is two XAML composition ticks after the first `Present`. It has not been compared with what reaches the screen.
- Against the plan's estimates (WinUI warm launch 200-350 ms, empty window 50-90 MB private): these proxies come in below both, at 126 ms and 30 MiB private. That is encouraging, but not a result until W0 repeats it on real hardware. This skeleton also has very little in it.
- Working set, at 117 MiB, is much larger than private bytes. It includes shared DLL pages, and the panel here is unusually large. A smaller window was not measured.
- About 20 s after a `see-it.sh` launch, idle, Windows reported 135.2 MB working set and 33.2 MB private bytes (one reading).

## Measured: runtime behaviour in the VM

**D3D11:** hardware device creation succeeded, with `adapter=Microsoft Basic Render Driver` in every trace. So the VM renders in software, but through the hardware path. **The WARP fallback exists in `render.rs` and was never taken, so it is untested.** The trace now records the driver and adapter.

**Controls.** I read `w1-out/run10/shot-final.png`, copied to `spikes/vm/winui-final.png`, and `winui-flyout.png`. They show a Windows 11 style title bar, File menu, two tabs with close buttons and `+`, tree, Find box and the panel. Mica isn't in this skeleton, and the app rendered in the light theme. At the viewer's downscale the checkerboard looks flat grey, which is why the check below uses pixels.

**Checkerboard, `spikes/vm/check_board.py`, on run 8, 9 and 10 screenshots:** in both `CopyFromScreen` and `PrintWindow` captures, the 1,939,705 pixels below the wheel band were all pure black or white in the correct phase (100.000%), with 0 wrong. Later captures report 95.429% only because the 40-row blue band has moved inside the measured area (92,920 pixels), and it has no effect on sharpness. That covers one scale only, 100%, where scaling is the easy case.

**Input** (`spikes/vm/smoke.ps1`, real SendInput into the app, in the RemoteApp session):

| Runs | Wheel | Lazy tree (expanded through UIA) | Flyout appears, click changes title | TextBox typing | File > Close exits, code 0 |
|---|---|---|---|---|---|
| 8, 9, 10, final build and harness | 3/3 | 3/3 | 3/3 | 3/3 | 3/3 |
| 4, 5, 6, 7, same build without the test flag, harness changed | 4/4 | 4/4 | 4/4 | 4/4 | 3/4 |
| 1, 2, first harness versions | 0/2 | 2/2 | 0/2 | 0/2 | 0/2 |

There is no run 3; the numbering skips it. That is 9 runs in all.

- **Wheel:** three events gave three `wheel delta` trace lines each, with deltas -120, -120, +120. The band moved from y=111 to y=151, matching the net two notches, in `check_band.py` output. Wheel events reach the panel through a transparent child `Grid`. A panel with no child was not tried.
- **Run 1 and 2 misses:** in run 1 the window wasn't foreground and `CopyFromScreen` failed with an invalid handle. In run 2 the window was foreground and all SendInput calls were accepted, yet the wheel, flyout and typing had no effect, and tree rows showed hover and selection away from where the script aimed. I then changed the harness (70 ms button hold, pauses, a UIA hit-test call first) and it worked. **I don't know why those two runs failed**: harness timing, RemoteApp focus and the host's real mouse are all possible. The wheel worked in all 7 later runs, but that isn't proof of reliability under real hands.
- **The Close miss in run 6:** the click came right after moving the cursor, with no pause. Adding 300 ms cured it in 3 of 3, which suggests hover timing but doesn't prove it.
- Not tested: theme switching, window resize, live scale change, real mouse and keyboard, and Tab key navigation.

**Failure path (defect 3), `spikes/vm/failpath.ps1`, using a `--test-fail-launch` flag:** one run. A message box appeared, which I read in `w1-out/shot-failbox.png`, and the process exited with code 1 once dismissed. With `--measure` it shows no box, writes `result=failed reason=...` and exits 1. Compose failure, panics and XAML's `UnhandledException` take the same `fatal` path, but those weren't triggered.

**DPI (defect 2):** both exes now embed PerMonitorV2 in their manifest. Traces read `DPI awareness=2` at 5-12 ms, as the first line after `main`. Only the self-contained exe was run.

**Launcher:** `spikes/winui/see-it.sh`. Tested once under `timeout 20`: a window titled `mdview WinUI skeleton` was running in the VM, which `Get-Process` confirmed (one check; I can't see the Linux side's window from here). Killing the RDP client left the app running in the disconnected session, and I ended it afterwards. Closing the window normally ends it. `C:\mdview-w0` holds the final build.

## Changes in this pass

- `ui.rs`: removed `Background` from `SwapChainPanel`; a transparent child `Grid` is the hit target. `build_rest` and tick failures are now fatal, so `--measure` can't report `ok` with controls missing.
- `main.rs`, `diag.rs`: `fatal` shows a message box (or logs in measure mode) and exits 1. Covers `OnLaunched`, compose, panics, and XAML unhandled exceptions. `OnLaunched` no longer returns an error to XAML, which caused the `0xc000027b` crash seen in Event Viewer on the first VM run.
- Manifests: PerMonitorV2 and Windows 10/11 `supportedOS` in both flavours; new `assets/framework.manifest`; the `SetProcessDpiAwarenessContext` call is gone. `filter.txt` and `bindings.rs` regenerated to match.
- `measure.ps1`: quoted path, tested from a folder with a space in 3 of 3 runs using the exact `W0.md` command; keeps `trace-runN.log`; reports exit codes and a missing log.
- `W0.md`: PowerShell `.\` launch, no-window steps, Task Manager check, copy trace first, missing scales fallback, Smart App Control.
- `spikes/vm/`: `w1.cmd`, `smoke`, `failpath`, `measure`, `leftover`, `diag`, `cleanup`, `check_board.py`, `check_band.py`. Output is in `w1-out/`.

**Reproduce in the VM:**
```bash
cd /home/kaan/markdown_viewer/spikes/winui && ./build-win.sh && ./stage.sh framework && ./stage.sh self-contained
timeout 90  ~/.local/bin/winapps manual '\\tsclient\home\markdown_viewer\spikes\vm\smoke.cmd'
timeout 170 ~/.local/bin/winapps manual '\\tsclient\home\markdown_viewer\spikes\vm\measure.cmd'
python3 ../vm/check_board.py ../vm/w1-out/shot-screen.png
```

**One unexplained hang:** the first `measure-run.ps1`, which called `measure.ps1` through a nested `powershell -File` with redirected output, had produced nothing when its 200 s timeout ended it. Calling it in-process, or through a child with `Start-Process`, took seconds. The cause wasn't found. `W0.md`'s command worked in the `Start-Process` form; typed interactively it is untested.

## What was built

- Raw `windows-bindgen 0.100.0` bindings over the Windows App SDK 2.6.4 winmds, via `gen/` and `filter.txt`, to `src/bindings.rs`. No XAML, C# or authored C++.
- Window with `TabView` and `SwapChainPanel` in the first frame, then `MenuBar`, `TreeView` (lazy `Expanding`), `TextBox` and tree `ContextFlyout` added after it.
- D3D11 device on a background thread. `CreateSwapChainForComposition`, 2 buffers, `FLIP_SEQUENTIAL`, `ALPHA_MODE_IGNORE`, inverse scale via `SetMatrixTransform`. A 1 px BGRA checkerboard through `UpdateSubresource`, and a band that follows `PointerWheelChanged`.
- `--measure[=FILE]` logs first-frame ms, full-UI ms, private bytes and working set, then exits, with a 45 s timeout. `--trace` writes startup checkpoints.
- Static CRT, so our exe does not import `vcruntime140.dll`.

**SDK version:** I used 2.6.4 rather than the newest 2.6.5, because the reg-free manifest and file list from windows-rs were checked against 2.6.4. The bootstrap minimum is 2.6.4, so an installed 2.6.5 also qualifies. The 2.6.5 download remains unused in `.cache/nuget`.

**Differences from upstream's self-contained assets:**
- The WebView2 entry is removed from the manifest, because no WebView2 projection is staged.
- `Microsoft.Graphics.Display.dll` is added to the staged list, because the manifest declares it and upstream's list omits it.

## Reproduce

```bash
cd /home/kaan/markdown_viewer/spikes/winui

# NuGet packages, only needed to regenerate bindings or stage
mkdir -p .cache/nuget264
for p in foundation winui interactiveexperiences runtime; do
  curl -sSfL -o .cache/nuget264/$p.nupkg https://api.nuget.org/v3-flatcontainer/microsoft.windowsappsdk.$p/2.6.4/microsoft.windowsappsdk.$p.2.6.4.nupkg
  unzip -qo .cache/nuget264/$p.nupkg -d .cache/nuget264/$p
done

# Bindings. The private toolchain has no rustfmt, so run gen with system rustfmt first on PATH.
(cd gen && env RUSTUP_HOME=$HOME/.local/rust-win/rustup CARGO_HOME=$HOME/.local/rust-win/cargo \
  PATH=$HOME/.local/rust-win/cargo/bin:$PATH cargo build -j 6 && PATH=/usr/bin:$PATH ./target/debug/gen)

./build-win.sh     # both flavours; uses XWIN_ACCEPT_LICENSE=true and .cache/xwin
./stage.sh framework
./stage.sh self-contained

# Wine, with a temporary prefix
export WINEPREFIX=$PWD/.wineprefix WINEARCH=win64 WINEDLLOVERRIDES="mscoree,mshtml=;winedbg.exe=d"
wineboot -i
cd stage/self-contained
WINEDEBUG=err+all,warn+module,+loaddll timeout 60 wine mdview-winui.exe --measure=measure.log --trace
wineserver -k
```

## Measured: build (noisy, parallel builds running)

| Item | Value | Label |
|---|---|---|
| Framework exe, manifest embedded | 378,880 bytes | measured |
| Self-contained exe, manifest embedded | 479,232 bytes | measured |
| `stage/self-contained` | 59 MiB, 28 DLLs plus language folders | measured (`du -sh`) |
| Embedded manifest | 915 of 915 activatable classes and 16 of 16 file entries match the source | measured |
| Framework exe imports | `api-ms-win-appmodel-runtime-l1-1-5`, `api-ms-win-core-synch-l1-2-0`, `api-ms-win-core-winrt-l1-1-0`, `api-ms-win-core-winrt-error-l1-1-0`, `d3d11`, `kernel32`, `ntdll`, `ole32`, `oleaut32`, `user32` | measured |
| Self-contained exe imports | The same, without the appmodel entry | measured |
| CRT and SDK download in `.cache/xwin` | 1.2 GiB | measured |

## Measured: Wine 11.20 (noisy)

**Plain Rust exe, `wine-smoke/`: works.** 3 of 3 runs exited 0, doing file I/O and console output. Wall time was 815, 6 and 6 ms. The later runs reuse a running wineserver.

**Self-contained WinUI exe: 5 runs, all ending at the same point, with exit 1. The framework build was run once.**

| Run | Settings | Wall time | Last trace line |
|---|---|---|---|
| 1 | Default, debugger on | 26,612 ms, mostly the debugger | `STA ready, calling Application::Start` |
| 2 | `winedbg` disabled, `+loaddll` | 839 ms | same |
| 3 | `winecfg -v win11` | 898 ms | same |
| 4, 5 | Win11, final binary | 1,129 and 841 ms | same |
| fw | Framework build, below | 809 ms | `main` |

(Runs 1-3 were an earlier binary, identical apart from a lint attribute and the extra staged DLL.)

How far it gets:

1. **Reached.** The exe loads, `main` runs at about 56-91 ms, the bootstrap is skipped, and the STA is set up.
2. **Reached.** The D3D11 device and DXGI factory were created on the background thread in the one run that lived long enough, at 279.5 ms. This is wined3d, so it says nothing about real Windows.
3. **Reached.** `Application::Start` is called. The reg-free manifest appears to resolve the class: `Microsoft.Internal.FrameworkUdk.dll`, `CoreMessagingXP.dll`, `Microsoft.UI.Windowing.Core.dll` and `Microsoft.ui.xaml.dll` all load as native DLLs.
4. **Fails.** A worker thread, not the main thread, aborts:
   ```
   wine: Call from 00006FFFFFC170CA to unimplemented function ntdll.dll.NtCreateWaitCompletionPacket, aborting
   ```
   `CoreMessagingXP.dll` is the only staged DLL that imports it.
5. **Never reached.** The Start callback, `Application::compose`, `OnLaunched` and the window.

**Framework build:** aborts right after `main` at `api-ms-win-appmodel-runtime-l1-1-5.dll.TryCreatePackageDependency`, which is unimplemented, as expected.

**Fixes tried:**
- Self-contained reg-free manifest: got it past bootstrap.
- `winedbg` override: removed the debugger delay only.
- Windows version set to win11: no change.
- winetricks: installed, but `list-all` shows no Windows App SDK or WinUI verb.

**Not tried:** stubbing the missing call by patching the import table, or rebuilding Wine's ntdll. That isn't a simple fix, and more walls are likely. `wine-logs/missing-imports.txt` lists 15 distinct imports with no Wine implementation in these four DLLs, including `GetWindowBand` and `IsWindowInDestroy`. Wine logs are in `wine-logs/`.

## Unverified, for W0

- Launch time and memory on real hardware with a GPU.
- 1 px sharpness at 150, 175 and 225%, and on a live scale change.
- Whether the staged file set is complete on a clean Windows 11. It was enough on this VM, which also has runtimes 1.5-1.8 installed, so this isn't a clean-machine test.
- Whether the first-frame marker matches what appears on screen.
- The framework-dependent build.
- The plan's "2.3.1 style optimizations", Mica, single-instance forwarding and theme repaint are not in this skeleton.

## Housekeeping

- The Wine prefix is deleted. No processes are left running.
- `.cache/` holds 2.6 GiB of downloads and is git-ignored. A `.cache/nuget` folder with unused 2.6.5 packages can be deleted by hand. An automatic safety check refused my removal of it, so I left it.

## Verification

Independent check on 2026-11-06, before the VM pass. Nothing in the spike was changed except this section. WinUI had not yet run anywhere, so the code review below comes from reading the code against Microsoft's docs and the package headers.

**Verdict at that time:** the cross-build part of G8 holds. **W0 should not go to the user yet.** Defect 1 below probably stops any window appearing, in both builds. (Superseded: see the status lines below and the VM results at the top.)

### Confirmed

| Check | Result |
|---|---|
| Clean rebuild | A source copy with no `target/` built both flavours with `build-win.sh` and `stage.sh`. Only `.cache` was reused. |
| Rebuilt exes against `stage/` | Same sizes (370,688 and 473,600 bytes). In each exe, 12 bytes differ, all in the PE and debug timestamps and PDB GUID. |
| Bindings | `gen` in the clean copy reproduced `src/bindings.rs` byte for byte. |
| Exe imports (`llvm-objdump`) | Match the table above. Only the framework exe imports `api-ms-win-appmodel-runtime-l1-1-5`, for `AddPackageDependency` and `TryCreatePackageDependency`. |
| Staged DLL closure, including delay imports | Everything not staged is an API set or a DLL expected in Windows 11 (judged by name, not checked on a PC). No `vcruntime`, `msvcp` or `concrt` imports. |
| Authored non-Rust code | None compiled. `Cargo.lock` has no `cc` or C++ dependency, and there are no C, C++, C#, XAML or IDL files. The non-Rust files are tooling and data: `build-win.sh`, `stage.sh`, `measure.ps1`, `filter.txt`, and the manifest copied from upstream. |
| Wine 11.20, one run, fresh prefix, win11, rebuilt self-contained exe | Same failure: exit 1, last trace line `STA ready, calling Application::Start`, then `unimplemented function ntdll.dll.NtCreateWaitCompletionPacket` on a worker thread. `CoreMessagingXP.dll` is the only staged DLL importing it. The prefix is deleted. A wineserver that was already running was left alone. |

### Defects in the WinUI code

Status after the VM pass: 1, 2 and 3 fixed and confirmed in the VM (1 was also confirmed at runtime as the real blocker). 4 and 5 were left alone: 4 caused no control failure in the VM, and 5 caused no failure, as the class is unused. The text below is the original review.

1. **Blocker: `src/ui.rs:71-78` sets `Background` on the `SwapChainPanel`.** Microsoft's [SwapChainPanel remarks](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.swapchainpanel) say "you can't set the Background on a SwapChainPanel. An error occurs if you attempt to set it." The call uses `?`, so `ui::build` returns before `Window::Activate` at line 139 and no window appears. This comes from the docs and hasn't been seen at runtime. Removing the call reopens the question the code was trying to answer: whether wheel events reach a panel with no background. A transparent child element may be needed.
2. **Medium: DPI awareness can silently fail.** `main.rs:72` starts the D3D thread before `SetProcessDpiAwarenessContext` at line 76, and the result is discarded. Upstream sets DPI first. Neither exe declares DPI in a manifest: the framework exe has no resource section, and the self-contained manifest has no `windowsSettings`. Microsoft recommends the manifest and says the API must be called before anything that depends on DPI awareness. If this fails, the window is bitmap-scaled and the 1 px test fails with nothing in the trace. Whether D3D creation can actually cause that is unknown.
3. **Medium: failures after `Application::Start` are nearly invisible.** Errors from `OnLaunched` and `Application::compose` (`main.rs:51,107`) only reach the trace, so a run without `--trace` shows no message at all. Without a window to close, the process may stay alive unseen. What XAML does when `OnLaunched` returns an error is unverified.
4. **Low: `XamlControlsXamlMetaDataProvider::Initialize` is neither bound nor called.** The package XML describes it as pre-activation logic. Upstream doesn't call it either, so this is only worth checking if control types fail to resolve.
5. **Low: the manifest declares `Microsoft.UI.Designer.dll`, which isn't staged.** The "16 of 16 file entries" row above compares the manifest with its source, not with the staged files. The only class is `DesignerOutputHost`, which the spike doesn't use, so this is harmless. It is the same kind of gap as the `Microsoft.Graphics.Display.dll` one fixed above.

**Read and found no defect:**
- STA set before `Start`, and swap chain work on the UI thread.
- `ISwapChainPanelNative` IID and vtable, against the 2.6.4 `microsoft.ui.xaml.media.dxinterop.h`.
- Package family and `MinVersion`, against the MSIX identity `Microsoft.WindowsAppRuntime.2` 2.6.4.0.
- `TryCreatePackageDependency` arguments and `HeapFree`.
- `IXamlMetadataProvider`, which matches upstream. A null type from the provider comes back as `S_OK` with no value written.
- `XamlControlsResources` merge, which matches upstream.
- Boxed strings, which implement `IPropertyValue` with type `String`.
- All 20 activated classes checked by name are in the manifest.
- Swap chain description, `SetMatrixTransform`, and `ResizeBuffers` with no buffers held.

### W0.md, as someone with only the build output

Status: items 1-6 are fixed in `W0.md` and `measure.ps1`. Only the space-in-path fix was run. The original review follows.

The runtime link works and lists a 2.6.4 x64 installer. These steps can still go wrong:

1. **Scales:** Settings only offers the scales a display supports, so 175% and 225% are often missing on smaller or lower-resolution screens. W0 gives no fallback. Custom scaling needs a sign-out.
2. **`measure.ps1` breaks on paths with spaces.** `-ArgumentList "--measure=$log"` isn't quoted, so a folder such as `C:\Users\First Last\...` gives no log and looks like an app failure.
3. **Launch command:** `mdview-winui.exe --trace` fails in PowerShell, which needs `.\`. W0 doesn't say which shell to use, and step 4 uses PowerShell.
4. **No-window case:** W0 doesn't say to check Task Manager for a leftover process and end it (see defect 3).
5. **Trace log overwritten:** each run replaces `mdview-winui.trace.log`, and `measure.ps1` starts ten runs. The log from a failed first launch is lost unless it's copied first.
6. **Smart App Control:** where it's on, it may block the unsigned exe, and `Unblock-File` won't help. W0 doesn't mention it.
