<#
.SYNOPSIS
  Builds mdview-qt in release, stages it with its Qt and MSVC runtime DLLs in target\qt-deploy,
  then builds the Inno Setup installer and its .sha256 file.
.NOTES
  pwsh packaging\windows\build-installer-qt.ps1 [-QtDir C:\Qt\6.9.3\msvc2022_64]
  Output: target\installer\mdview-qt-setup-<version>.exe and .exe.sha256
#>
param([string]$QtDir = 'C:\Qt\6.9.3\msvc2022_64')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function Fail($msg) { Write-Error $msg; exit 1 }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$toml = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$m = [regex]::Match($toml, '(?ms)^\[workspace\.package\]\s*(?:(?!^\[).)*?^version\s*=\s*"([^"]+)"')
if (-not $m.Success) { Fail 'Could not read [workspace.package] version from Cargo.toml.' }
$version = $m.Groups[1].Value

if (-not (Test-Path -LiteralPath "$QtDir\bin\windeployqt.exe")) { Fail "No windeployqt.exe under $QtDir\bin." }
$iscc = $null; $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue; if ($cmd) { $iscc = $cmd.Source }
if (-not $iscc) {
  foreach ($c in "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 7\ISCC.exe",
                 "$env:ProgramFiles\Inno Setup 7\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") {
    if (Test-Path -LiteralPath $c) { $iscc = $c; break }
  }
}
if (-not $iscc) { Fail 'ISCC.exe not found.' }

# The MSVC runtime, shipped app-local next to the Qt DLLs (newest Visual Studio redist found).
$crt = Get-ChildItem -Directory -Recurse -Depth 8 -ErrorAction SilentlyContinue `
         "$env:ProgramFiles\Microsoft Visual Studio", "${env:ProgramFiles(x86)}\Microsoft Visual Studio" |
       Where-Object { $_.Name -like 'Microsoft.VC14*.CRT' -and $_.Parent.Name -eq 'x64' -and $_.FullName -notmatch 'onecore' } |
       Sort-Object FullName -Descending | Select-Object -First 1
if (-not $crt) { Fail 'MSVC x64 CRT redist folder not found.' }

$env:PATH = "$QtDir\bin;$env:PATH"; $env:QMAKE = "$QtDir\bin\qmake.exe"; $env:CMAKE_PREFIX_PATH = $QtDir
$tmp = Join-Path $root 'target\tmp'; New-Item -ItemType Directory -Force $tmp | Out-Null
$env:TEMP = $tmp; $env:TMP = $tmp   # CXX-Qt's build fails under paths it cannot write

cargo build --release --manifest-path (Join-Path $root 'bins\mdv-qt\Cargo.toml')
if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed.' }

$stage = Join-Path $root 'target\qt-deploy'
if (Test-Path -LiteralPath $stage) { Remove-Item -Recurse -Force -LiteralPath $stage }
New-Item -ItemType Directory $stage | Out-Null
Copy-Item (Join-Path $root 'bins\mdv-qt\target\release\mdview-qt.exe') $stage
Copy-Item (Join-Path $root 'assets\mdview.ico') $stage
& "$QtDir\bin\windeployqt.exe" --release --no-translations --no-system-d3d-compiler --no-opengl-sw --no-compiler-runtime "$stage\mdview-qt.exe"
if ($LASTEXITCODE -ne 0) { Fail 'windeployqt failed.' }
foreach ($f in 'msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll') {
  Copy-Item (Join-Path $crt.FullName $f) $stage
}

& $iscc "/DAppVersion=$version" (Join-Path $PSScriptRoot 'mdview-qt.iss')
if ($LASTEXITCODE -ne 0) { Fail 'ISCC failed.' }
$setup = Join-Path $root "target\installer\mdview-qt-setup-$version.exe"
$hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
[System.IO.File]::WriteAllText("$setup.sha256", "$hash  mdview-qt-setup-$version.exe`n", [System.Text.UTF8Encoding]::new($false))
Write-Host "Built $setup"
Write-Host "SHA256 $hash"
