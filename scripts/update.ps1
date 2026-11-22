$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$manual = $env:QUICKPEEK_UPDATE_MANUAL -eq '1'
function Notify([string]$message, [int]$flags = 64) {
    (New-Object -ComObject WScript.Shell).Popup($message, 0, 'QuickPeek', ($flags -bor 4096))
}
function AutoEnabled {
    (Get-ItemProperty 'HKCU:\Software\Nex-Z\QuickPeek' -ErrorAction SilentlyContinue).AutoUpdate -eq 1
}
function Get-UpdateAsset($release, [version]$current) {
    if ($release.draft -or $release.prerelease -or $release.tag_name -cnotmatch '^v(\d+\.\d+\.\d+)$') { return }
    $versionText = $Matches[1]
    if ([version]$versionText -le $current) { return }
    $name = "QuickPeek-$versionText-windows-x64-setup.exe"
    $expected = "https://github.com/Nex-Z/QuickPeek/releases/download/v$versionText/$name"
    $assets = @($release.assets | Where-Object { $_.name -ceq $name -and $_.browser_download_url -ceq $expected })
    if ($assets.Count -ne 1 -or $assets[0].digest -notmatch '^sha256:[a-f0-9]{64}$') { throw '发布包或 GitHub SHA-256 摘要缺失。' }
    $assets[0]
}
function Install-Update($asset, $exe, $broker) {
    $stage = Join-Path ([IO.Path]::GetTempPath()) ('QuickPeek-update-' + [Guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($stage)
    $restart = $false
    try {
        $setup = Join-Path $stage 'setup.exe'
        Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $setup -TimeoutSec 300
        $sha256 = [Security.Cryptography.SHA256]::Create()
        try {
            $stream = [IO.File]::OpenRead($setup)
            try {
                $actual = 'sha256:' + ([BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToLowerInvariant())
            } finally { $stream.Dispose() }
        } finally { $sha256.Dispose() }
        if ($actual -ne $asset.digest) { throw '下载文件的 SHA-256 校验失败。' }
        if (!$manual -and !(AutoEnabled)) { return }
        Add-Type @"
using System;
using System.Runtime.InteropServices;
public class QuickPeekUpdateWindow {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls,string title);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,IntPtr w,IntPtr l);
}
"@
        $window = [QuickPeekUpdateWindow]::FindWindow('QuickPeek.Broker.v1','QuickPeek')
        [uint32]$owner = 0
        [void][QuickPeekUpdateWindow]::GetWindowThreadProcessId($window,[ref]$owner)
        if ($broker.HasExited -or $owner -ne $broker.Id -or $broker.Path -ne $exe) { return }
        $others = @(Get-Process -Name quickpeek -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $broker.Id -and $_.Path -eq $exe })
        if (@($others | Where-Object MainWindowHandle -ne 0).Count) {
            if ($manual) { [void](Notify '请先关闭预览窗口，再检查更新。') }
            return
        }
        $dir = Split-Path -LiteralPath $exe
        $argsList = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOCLOSEAPPLICATIONS',('/DIR="' + $dir + '"'))
        $uninstall = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Nex-Z.QuickPeek_is1'
        $installed = (Get-ItemProperty $uninstall -ErrorAction SilentlyContinue).InstallLocation
        if (!$installed -or $installed.TrimEnd('\') -ne $dir.TrimEnd('\')) { $argsList += '/MODE=portable' }
        if (![QuickPeekUpdateWindow]::PostMessage($window,16,[IntPtr]::Zero,[IntPtr]::Zero)) { throw '无法退出托盘。' }
        if (!$broker.WaitForExit(15000)) { throw '托盘未及时退出。' }
        $restart = $true
        foreach ($other in $others) {
            if (!$other.HasExited -and !$other.WaitForExit(5000)) { throw '预览仍在使用中，更新已推迟。' }
        }
        $process = Start-Process -FilePath $setup -ArgumentList $argsList -PassThru -Wait
        if ($process.ExitCode) { throw "安装程序退出码 $($process.ExitCode)" }
    } finally {
        if ($restart) { Start-Process -FilePath $exe -WindowStyle Hidden }
        if (Test-Path -LiteralPath (Join-Path $stage 'setup.exe')) { [IO.File]::Delete((Join-Path $stage 'setup.exe')) }
        [IO.Directory]::Delete($stage)
    }
}
function Invoke-QuickPeekUpdate {
    if (!$manual -and !(AutoEnabled)) { return }
    $broker = Get-Process -Id ([int]$env:QUICKPEEK_UPDATE_PID)
    $current = [version]$env:QUICKPEEK_UPDATE_VERSION
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/Nex-Z/QuickPeek/releases/latest' -Headers @{'User-Agent'='QuickPeek-Updater'; Accept='application/vnd.github+json'} -TimeoutSec 30
    $asset = Get-UpdateAsset $release $current
    if (!$asset) {
        if ($manual) { [void](Notify "当前版本 $current 已是最新稳定版。") }
        return
    }
    if ($manual -and (Notify "发现 $($release.tag_name)，现在下载并安装？" 36) -ne 6) { return }
    Install-Update $asset $env:QUICKPEEK_UPDATE_EXE $broker
}
if ($MyInvocation.InvocationName -ne '.') {
    try { Invoke-QuickPeekUpdate } catch {
        if ($manual) { [void](Notify ("更新失败：" + $_.Exception.Message) 16) }
        else { [Diagnostics.Trace]::WriteLine($_.Exception.Message) }
        exit 1
    }
}
