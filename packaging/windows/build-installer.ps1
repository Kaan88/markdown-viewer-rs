<#
.SYNOPSIS
  Builds mdview and mdview-engine in release (static C runtime, no DLLs to ship), then builds the
  Inno Setup installer and its .sha256 file, and puts the files of a GitHub release together.
.NOTES
  pwsh packaging\windows\build-installer.ps1 [-SkipBuild] [-Notes 'One line of release notes.']
  Output: target\installer\mdview-setup-<version>.exe and .exe.sha256, and target\release-files\
  with what a release v<version> carries: the installer, mdview.exe, mdview-engine.exe and
  mdview-update.txt (the manifest an installed mdview reads to update itself; docs\UPDATE.md).
#>
param([switch]$SkipBuild, [string]$Notes = '')
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
  # set here and not in .cargo\config.toml because it must not reach a Linux build; a separate
  # target directory keeps normal builds from rebuilding.
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

# The files of the release. The manifest's URLs are where GitHub serves the assets of tag v<version>.
$dist = Join-Path $root 'target\dist\release'
$files = Join-Path $root 'target\release-files'
if (Test-Path -LiteralPath $files) { Remove-Item -LiteralPath $files -Recurse -Force }
New-Item -ItemType Directory -Force $files | Out-Null
Copy-Item -LiteralPath $setup, (Join-Path $dist 'mdview.exe'), (Join-Path $dist 'mdview-engine.exe') -Destination $files
$base = "https://github.com/Kaan88/markdown-viewer-rs/releases/download/v$version"
$sha = { param($f) (Get-FileHash -LiteralPath (Join-Path $files $f) -Algorithm SHA256).Hash.ToLowerInvariant() }
$lines = @('manifest: 1', "version: $version")
if ($Notes) {
  if ($Notes -match '[\x00-\x1f]' -or [System.Text.Encoding]::UTF8.GetByteCount($Notes) -gt 1024) { Fail 'Notes must be one line of at most 1024 bytes.' }
  $lines += "notes: $Notes"
}
$lines += @(
  'windows-x86_64.asset: mdview.exe',
  "windows-x86_64.url: $base/mdview.exe",
  "windows-x86_64.sha256: $(& $sha 'mdview.exe')",
  "windows-x86_64.engine.url: $base/mdview-engine.exe",
  "windows-x86_64.engine.sha256: $(& $sha 'mdview-engine.exe')"
)
[System.IO.File]::WriteAllText((Join-Path $files 'mdview-update.txt'), (($lines -join "`n") + "`n"), [System.Text.UTF8Encoding]::new($false))
Write-Host "Release files in $files"
