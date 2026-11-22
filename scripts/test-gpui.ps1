param(
    [Parameter(Mandatory)][string]$Exe,
    [string[]]$Files = @('tests/fixtures/example.txt', 'tests/fixtures/example.rs', 'tests/fixtures/example.json', 'tests/fixtures/invalid.json', 'tests/fixtures/prototype.png', 'tests/fixtures/sample.pdf'),
    [string]$Output = 'target/gpui-smoke',
    [switch]$Classic
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$exePath = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force $Output | Out-Null
foreach ($file in $Files) {
    $path = (Resolve-Path $file).Path
    $mode = if ($Classic) { '--classic' } else { '--preview' }
    $checkTitle = !$Classic -and [IO.Path]::GetExtension($file) -in '.txt','.rs','.json','.png','.log'
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $exePath -ArgumentList @($mode, ('"' + $path + '"')) -PassThru
    try {
        do {
            $process.Refresh()
            if ($process.HasExited) { throw "Process exited early ($($process.ExitCode)) for $file" }
            if ($process.MainWindowHandle -and (!$checkTitle -or $process.MainWindowTitle -eq [IO.Path]::GetFileName($file))) { break }
            Start-Sleep -Milliseconds 15
        } while ($clock.ElapsedMilliseconds -lt 20000)
        if (!$process.MainWindowHandle -or ($checkTitle -and $process.MainWindowTitle -ne [IO.Path]::GetFileName($file))) {
            throw "Preview window missing for $file ($($process.MainWindowTitle))"
        }
        $windowMs = $clock.ElapsedMilliseconds
        Start-Sleep -Milliseconds 650
        $screenshot = Join-Path $Output "$([IO.Path]::GetFileName($file)).png"
        for ($attempt = 0; $attempt -lt 12; $attempt++) {
            try {
                & "$PSScriptRoot/inspect-window.ps1" -ProcessId $process.Id -Output $screenshot | Out-Null
                break
            } catch {
                if ($attempt -eq 11 -or $process.HasExited) { throw }
                Start-Sleep -Milliseconds 250
            }
        }
        if (-not (Test-Path $screenshot)) { throw "Screenshot missing for $file" }
        [pscustomobject]@{File=$file; WindowMs=$windowMs; WorkingSetMiB=[math]::Round($process.WorkingSet64/1MB,1); Title=$process.MainWindowTitle; Screenshot=$screenshot}
    } finally {
        if (-not $process.HasExited) {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(8000)) { Write-Warning "Window still open (PID $($process.Id))" }
        }
    }
}
