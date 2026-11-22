param([switch]$SkipBuild, [string]$OutputDirectory = 'dist')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if (!$env:GPUI_FXC_PATH -and !(Get-Command fxc.exe -ErrorAction SilentlyContinue)) {
    $kits = Get-ItemPropertyValue 'HKLM:/SOFTWARE/WOW6432Node/Microsoft/Windows Kits/Installed Roots' KitsRoot10
    $fxc = Get-ChildItem "$kits/bin/*/x64/fxc.exe" | Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
    if (!$fxc) { throw 'Install the Windows SDK x64 shader compiler or set GPUI_FXC_PATH.' }
    $env:GPUI_FXC_PATH = $fxc.FullName
}
if (-not $SkipBuild) {
    cargo test
    if ($LASTEXITCODE) { throw 'Tests failed' }
    cargo clippy --all-targets -- -D warnings
    if ($LASTEXITCODE) { throw 'Clippy failed' }
    cargo build --release --locked
    if ($LASTEXITCODE) { throw 'Release build failed' }
}
$metadata = cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc --locked | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'Cargo metadata failed' }
$version = ($metadata.packages | Where-Object name -eq quickpeek).version
$name = "QuickPeek-$version-windows-x64"
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$releaseRoot = Join-Path $metadata.target_directory 'release'
$destination = Join-Path $outputRoot $name
if (Test-Path $destination) {
    Move-Item -LiteralPath $destination -Destination "$destination-backup-$(Get-Date -Format yyyyMMdd-HHmmss)"
}
New-Item -ItemType Directory -Force $destination | Out-Null
$runtime = $metadata.packages | Where-Object name -eq windows-reactor-setup
$runtimeList = Join-Path (Split-Path $runtime.manifest_path) 'assets/runtime.txt'
foreach ($entry in Get-Content $runtimeList) {
    $entry = $entry.Trim()
    if (-not $entry) { continue }
    $source = Join-Path $releaseRoot $entry
    if (Test-Path $source) { Copy-Item -LiteralPath $source -Destination $destination -Recurse }
}
Copy-Item (Join-Path $releaseRoot 'quickpeek.exe') $destination
# WinUI projection dependency; PDF uses the system WebView2 Runtime.
Copy-Item (Join-Path $releaseRoot 'Microsoft.Web.WebView2.Core.dll') $destination
$vswhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
if (Test-Path $vswhere) {
    $vs = & $vswhere -latest -products '*' -property installationPath
    $redist = Get-ChildItem "$vs/VC/Redist/MSVC" -Directory | Where-Object Name -match '^\d' | Sort-Object Name -Descending | Select-Object -First 1
    $crt = Get-ChildItem "$($redist.FullName)/x64" -Directory | Where-Object Name -like '*.CRT' | Select-Object -First 1
    if ($crt) { Copy-Item "$($crt.FullName)/*.dll" $destination }
}
foreach ($required in 'Microsoft.UI.Xaml.dll','Microsoft.WindowsAppRuntime.dll','msvcp140.dll','vcruntime140.dll') {
    if (-not (Test-Path (Join-Path $destination $required))) { throw "Missing runtime: $required" }
}
Copy-Item README.md $destination
Copy-Item docs (Join-Path $destination 'docs') -Recurse
# Evidence images are development artifacts; only ship the documentation files.
$evidence = Join-Path $destination 'docs/evidence'
if (Test-Path $evidence) {
    $resolved = [IO.Path]::GetFullPath($evidence)
    if (-not $resolved.StartsWith([IO.Path]::GetFullPath($destination) + [IO.Path]::DirectorySeparatorChar)) { throw 'Invalid evidence path' }
    Remove-Item -LiteralPath $resolved -Recurse
}
$licenses = Join-Path $destination 'licenses'
New-Item -ItemType Directory -Force $licenses | Out-Null
$summary = @('# Third-party dependencies', '', 'QuickPeek uses Windows App SDK and Microsoft Visual C++ Redistributable runtime components.', 'Runtime terms: https://www.nuget.org/packages/Microsoft.WindowsAppSDK.Runtime/2.4.0', '')
$summary += 'highlight.js 11.11.1: BSD-3-Clause (bundled offline browser syntax highlighting)'
Copy-Item -LiteralPath 'assets/vendor/highlight.LICENSE' -Destination (Join-Path $licenses 'highlight.js-LICENSE.txt')
foreach ($package in $metadata.packages | Where-Object name -ne quickpeek) {
    $summary += "$($package.name) $($package.version): $($package.license)"
    $packageDir = Split-Path $package.manifest_path
    foreach ($file in Get-ChildItem $packageDir -File | Where-Object Name -match '^(LICENSE|COPYING|NOTICE|COPYRIGHT)') {
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $licenses "$($package.name)-$($package.version)-$($file.Name)")
    }
}
$summary | Set-Content (Join-Path $licenses 'DEPENDENCIES.txt') -Encoding UTF8
cargo run --quiet --release --example third_party | Set-Content (Join-Path $licenses 'SYNTAX-ACKNOWLEDGEMENTS.txt') -Encoding UTF8
if ($LASTEXITCODE) { throw 'Could not collect syntax licenses' }
$zip = Join-Path $outputRoot "$name.zip"
if (Test-Path $zip) { Move-Item -LiteralPath $zip -Destination "$zip.backup-$(Get-Date -Format yyyyMMdd-HHmmss)" }
# NuGet ships some files with pre-1980 timestamps, outside the ZIP format range.
Get-ChildItem $destination -Recurse -Force | ForEach-Object {
    if ($_.LastWriteTime.Year -lt 1980) { $_.LastWriteTime = [datetime]'1980-01-01' }
}
Compress-Archive -Path $destination -DestinationPath $zip -CompressionLevel Optimal
$hash = Get-FileHash -Algorithm SHA256 $zip
"$($hash.Hash.ToLower())  $name.zip" | Set-Content "$zip.sha256" -Encoding ascii
Get-Item $zip | Select-Object FullName, Length
Write-Output "SHA256: $($hash.Hash)"

