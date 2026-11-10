<#
.SYNOPSIS
  Builds the release binaries and the Inno Setup installer, then writes the .sha256 file.
.NOTES
  Run on Windows from anywhere:  pwsh packaging\windows\build-installer.ps1
  Output: target\installer\mdview-setup-<version>.exe and .exe.sha256
#>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail($msg) { Write-Error $msg; exit 1 }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail 'cargo not found on PATH (install Rust from https://rustup.rs).' }

# Version = [workspace.package] version in the workspace Cargo.toml.
$toml = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$m = [regex]::Match($toml, '(?ms)^\[workspace\.package\]\s*(?:(?!^\[).)*?^version\s*=\s*"([^"]+)"')
if (-not $m.Success) { Fail 'Could not read [workspace.package] version from Cargo.toml.' }
$version = $m.Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { Fail "Unexpected version '$version'." }

# Locate ISCC.exe: PATH, then the usual install locations (Inno Setup 7, then 6).
$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
  $dirs = @($env:ProgramFiles, ${env:ProgramFiles(x86)}, "$env:LOCALAPPDATA\Programs") | Where-Object { $_ }
  foreach ($d in $dirs) {
    foreach ($v in 'Inno Setup 7', 'Inno Setup 6') {
      $c = Join-Path $d "$v\ISCC.exe"
      if (Test-Path -LiteralPath $c) { $iscc = $c; break }
    }
    if ($iscc) { break }
  }
}
if (-not $iscc) { Fail 'ISCC.exe not found. Install Inno Setup (https://jrsoftware.org/isinfo.php) or add it to PATH.' }

Push-Location $root
try {
  cargo build --release -p mdview-winui -p mdview-engine
  if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed.' }
  foreach ($f in 'mdview-winui.exe', 'mdview-engine.exe') {
    if (-not (Test-Path -LiteralPath "target\release\$f")) { Fail "Missing target\release\$f after the build." }
  }

  & $iscc "/DAppVersion=$version" (Join-Path $PSScriptRoot 'mdview.iss')
  if ($LASTEXITCODE -ne 0) { Fail 'ISCC failed.' }

  $setup = Join-Path $root "target\installer\mdview-setup-$version.exe"
  if (-not (Test-Path -LiteralPath $setup)) { Fail "Expected installer not found: $setup" }
  $hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
  # "<hash>  <name>" is the format sha256sum -c understands.
  [System.IO.File]::WriteAllText("$setup.sha256", "$hash  mdview-setup-$version.exe`n", [System.Text.UTF8Encoding]::new($false))
  Write-Host "Built $setup"
  Write-Host "SHA256 $hash"
} finally { Pop-Location }
