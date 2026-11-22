param([string]$Exe = 'target/release/quickpeek.exe',[switch]$KeyboardOnly,[switch]$UiOnly)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Windows.Forms
Add-Type @'
using System;using System.Runtime.InteropServices;
public class PeekNativeTest {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int Size; public RECT Monitor,Work; public uint Flags; }
  [DllImport("user32.dll")]public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out RECT r);
  [DllImport("user32.dll")]public static extern IntPtr MonitorFromWindow(IntPtr h,uint flags);
  [DllImport("user32.dll")]public static extern bool GetMonitorInfo(IntPtr h,ref MONITORINFO i);
  [DllImport("user32.dll")]public static extern int GetWindowLong(IntPtr h,int index);
  [DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);
  [DllImport("user32.dll")]public static extern IntPtr FindWindow(string c,string t);
  [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
}
'@
[void][PeekNativeTest]::SetThreadDpiAwarenessContext([IntPtr](-4))
$exePath = [IO.Path]::GetFullPath($Exe)
function Key([byte]$Code) {
    [PeekNativeTest]::keybd_event($Code,0,0,[UIntPtr]::Zero)
    [PeekNativeTest]::keybd_event($Code,0,2,[UIntPtr]::Zero)
}
function Activate($Handle) {
    if ([PeekNativeTest]::GetForegroundWindow() -eq $Handle) { Start-Sleep -Milliseconds 250; return }
    Key 18
    if (-not [PeekNativeTest]::SetForegroundWindow($Handle)) { throw 'Could not activate test window' }
    Start-Sleep -Milliseconds 250
}
function Wait-Until([scriptblock]$Condition,[string]$Message) {
    for ($i=0; $i -lt 60; $i++) { if (& $Condition) { return }; Start-Sleep -Milliseconds 100 }
    throw $Message
}
function Names($Process) {
    $Process.Refresh()
    $root=[System.Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
    $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {$_.Current.Name}
}
function Invoke-Control($Process,[string]$Name) {
    $root=[System.Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
    $c=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
    foreach($el in $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$c)) {
        $pattern=$null
        if($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) {$pattern.Invoke();return}
        if($el.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern,[ref]$pattern)) {$pattern.Expand();return}
    }
    throw "Missing actionable control: $Name"
}
$results=@()
$cases=@(
    @{Path='F:\Download\ChatGPT Image 2026年9月22日 15_32_00.png'; Kind='image'},
    @{Path='F:\Download\PixPin_2026-09-14_23-10-19.webp'; Kind='image'},
    @{Path='F:\Download\playlist-file.wav'; Kind='media'},
    @{Path=[IO.Path]::GetFullPath('tests/fixtures/sample.mp4'); Kind='media'}
)
foreach($case in $cases) {
    $process=Start-Process $exePath -ArgumentList @('--classic',('"'+$case.Path+'"')) -WindowStyle Hidden -PassThru
    try {
        Wait-Until {$process.Refresh();$process.MainWindowHandle -ne 0} 'Window missing'
        Wait-Until {-not ((Names $process) -contains '正在加载…')} 'Load timed out'
        Wait-Until {[PeekNativeTest]::GetForegroundWindow() -eq $process.MainWindowHandle} 'Preview did not activate'
        if(([PeekNativeTest]::GetWindowLong($process.MainWindowHandle,-20) -band 8) -ne 0){throw 'Default topmost unexpected'}
        if($case.Kind -eq 'image') {
            Wait-Until {((Names $process) -join '|') -match '图片 ·'} 'Image not decoded'
            if(((Names $process) -join '|') -match '无法解码'){throw 'Image decode failed'}
            if((Names $process) -contains '适应宽度'){throw 'Image mode toolbar should be removed'}
        } else {
            Wait-Until {((Names $process) -join '|') -match '00:0[1-9] / 00:'} 'Autoplay position did not advance'
            Invoke-Control $process '暂停'
            Wait-Until {(Names $process) -contains '播放'} 'Pause failed'
            Invoke-Control $process '播放'
        }
        $results += "$([IO.Path]::GetFileName($case.Path)): PASS"
    } finally { if(-not $process.HasExited){[void]$process.CloseMainWindow();[void]$process.WaitForExit(3000)} }
}
# Replace only the idle resident broker for the test; do not close user previews.
$existing=[PeekNativeTest]::FindWindow('QuickPeek.Broker.v1','QuickPeek')
if($existing -ne [IntPtr]::Zero){[void][PeekNativeTest]::PostMessage($existing,16,[IntPtr]::Zero,[IntPtr]::Zero);Start-Sleep -Milliseconds 400}
$broker=Start-Process $exePath -WindowStyle Hidden -PassThru
$opened=@()
try {
    Start-Process explorer.exe -ArgumentList ('/select,'+[IO.Path]::GetFullPath('tests/fixtures/example.json'))
    $shell=New-Object -ComObject Shell.Application
    $uri=([Uri][IO.Path]::GetFullPath('tests/fixtures')).AbsoluteUri
    Wait-Until {$script:explorer=$shell.Windows()|Where-Object {$_.LocationURL.TrimEnd('/') -eq $uri.TrimEnd('/')}|Select-Object -Last 1; $null -ne $script:explorer} 'Explorer not ready'
    foreach($file in 'example.json','prototype.png') {
        $before=@(Get-Process quickpeek -ErrorAction SilentlyContinue | Where-Object MainWindowHandle -ne 0 | ForEach-Object Id)
        $script:explorer.Document.SelectItem($script:explorer.Document.Folder.ParseName($file),29)
        Activate ([IntPtr]$script:explorer.HWND)
        Start-Sleep -Milliseconds 300
        Key 32
        Wait-Until {$script:preview=Get-Process quickpeek -ErrorAction SilentlyContinue|Where-Object {$_.Id -notin $before -and $_.MainWindowHandle -ne 0}|Select-Object -First 1;$null -ne $script:preview} 'Space did not open next preview'
        $opened += $script:preview
        Wait-Until {[PeekNativeTest]::GetForegroundWindow() -eq $script:preview.MainWindowHandle} 'Space preview was not foreground'
        if(([PeekNativeTest]::GetWindowLong($script:preview.MainWindowHandle,-20) -band 8) -ne 0){throw 'Space preview unexpectedly topmost'}
    }
    if($opened[0].HasExited){throw 'Existing preview unexpectedly closed'}
    $results+='Explorer Space twice with existing preview + foreground without topmost: PASS'
} finally {
    foreach($process in $opened){if(-not $process.HasExited){[void]$process.CloseMainWindow();[void]$process.WaitForExit(3000)}}
    $hwnd=[PeekNativeTest]::FindWindow('QuickPeek.Broker.v1','QuickPeek')
    if($hwnd -ne [IntPtr]::Zero){[void][PeekNativeTest]::PostMessage($hwnd,16,[IntPtr]::Zero,[IntPtr]::Zero)}
    [void]$broker.WaitForExit(3000)
}
$results | Set-Content 'docs/evidence/regression-tests.txt' -Encoding UTF8
$results
