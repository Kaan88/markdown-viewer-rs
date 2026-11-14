<#
.SYNOPSIS
  Makes the portable zip of the release: mdview-<version>-windows-x86_64.zip, one folder holding
  mdview.exe, mdview-engine.exe, LICENSE and README.md (the viewer starts the engine from beside
  itself, so the two exes stay together). Unzip it anywhere and run mdview.exe; it updates itself in
  place like the installed copy (the folder must be writable).
.NOTES
  pwsh packaging\windows\build-zip.ps1 [-Dir folder-with-both-exes] [-OutDir dir]
  Defaults: the exes in target\dist\release, the zip into target\release-files. Run after
  packaging\windows\build-installer.ps1.
#>
param([string]$Dir = '', [string]$OutDir = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function Fail($msg) { Write-Error $msg; exit 1 }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$toml = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$m = [regex]::Match($toml, '(?ms)^\[workspace\.package\]\s*(?:(?!^\[).)*?^version\s*=\s*"([^"]+)"')
if (-not $m.Success) { Fail 'Could not read [workspace.package] version from Cargo.toml.' }
$version = $m.Groups[1].Value
if (-not $Dir) { $Dir = Join-Path $root 'target\dist\release' }
if (-not $OutDir) { $OutDir = Join-Path $root 'target\release-files' }
foreach ($f in 'mdview.exe', 'mdview-engine.exe') {
  if (-not (Test-Path -LiteralPath (Join-Path $Dir $f))) { Fail "$Dir\$f is missing." }
}
New-Item -ItemType Directory -Force $OutDir | Out-Null

$name = "mdview-$version-windows-x86_64"
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ("zip-" + [guid]::NewGuid().ToString('N'))
$folder = Join-Path $stage $name
New-Item -ItemType Directory -Force $folder | Out-Null
Copy-Item -LiteralPath (Join-Path $Dir 'mdview.exe'), (Join-Path $Dir 'mdview-engine.exe') -Destination $folder
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $folder
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $folder

$zip = Join-Path $OutDir "$name.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
# Entries with forward slashes, as the zip format wants; Compress-Archive of Windows PowerShell 5
# writes backslashes, so the archive is built by hand.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::Open($zip, 'Create')
try {
  foreach ($f in Get-ChildItem -LiteralPath $folder -File | Sort-Object Name) {
    [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $f.FullName, "$name/$($f.Name)", 'Optimal')
  }
} finally { $archive.Dispose() }
Remove-Item -LiteralPath $stage -Recurse -Force
Write-Host "built $zip ($((Get-Item $zip).Length) bytes)"
