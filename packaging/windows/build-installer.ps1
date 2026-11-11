<#
.SYNOPSIS
  Builds mdview and mdview-engine in release (static C runtime, no DLLs to ship), then builds the
  Inno Setup installer and its .sha256 file.
.NOTES
  pwsh packaging\windows\build-installer.ps1 [-SkipBuild]
  Output: target\installer\mdview-setup-<version>.exe and .exe.sha256
#>
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function Fail($msg) { Write-Error $msg; exit 1 }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$toml = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$m = [regex]::Match($toml, '(?ms)^\[workspace\.package\]\s*(?:(?!^\[).)*?^version\s*=\s*"([^"]+)"')
if (-not $m.Success) { Fail 'Could not read [workspace.package] version from Cargo.toml.' }
$version = $m.Groups[1].Value

$iscc = $null; $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue; if ($cmd) { $iscc = $cmd.Source }
if (-not $iscc) {
  foreach ($c in "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 7\ISCC.exe",
                 "$env:ProgramFiles\Inno Setup 7\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") {
    if (Test-Path -LiteralPath $c) { $iscc = $c; break }
  }
}
if (-not $iscc) { Fail 'ISCC.exe not found.' }

if (-not $SkipBuild) {
  $tmp = Join-Path $root 'target\tmp'; New-Item -ItemType Directory -Force $tmp | Out-Null
  $env:TEMP = $tmp; $env:TMP = $tmp   # lib.exe and rc.exe fail under paths they cannot write
  # The shipped exes link the C runtime statically, so they need no VC++ redistributable. The flag is
  # set here and not in .cargo\config.toml because it must not reach the Qt crate (Qt needs the
  # dynamic CRT) or a Linux build; a separate target directory keeps normal builds from rebuilding.
  $env:RUSTFLAGS = '-C target-feature=+crt-static'
  Push-Location $root
  try { cargo build --release -p mdview -p mdview-engine --target-dir target\dist } finally { Pop-Location; Remove-Item Env:RUSTFLAGS }
  if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed.' }
}
foreach ($f in 'mdview.exe', 'mdview-engine.exe') {
  if (-not (Test-Path -LiteralPath (Join-Path $root "target\dist\release\$f"))) { Fail "target\dist\release\$f is missing." }
}

& $iscc "/DAppVersion=$version" (Join-Path $PSScriptRoot 'mdview.iss')
if ($LASTEXITCODE -ne 0) { Fail 'ISCC failed.' }
$setup = Join-Path $root "target\installer\mdview-setup-$version.exe"
$hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
[System.IO.File]::WriteAllText("$setup.sha256", "$hash  mdview-setup-$version.exe`n", [System.Text.UTF8Encoding]::new($false))
Write-Host "Built $setup"
Write-Host "SHA256 $hash"
