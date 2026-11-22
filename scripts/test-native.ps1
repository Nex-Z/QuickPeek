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
foreach($file in 'readme.md','example.txt','example.rs','example.json','invalid.json','prototype.png','sample.pdf','sample.mp4') {
    if ($KeyboardOnly) { break }
    $path=[IO.Path]::GetFullPath("tests/fixtures/$file")
    $process=Start-Process -FilePath $exePath -ArgumentList @('--classic',('"'+$path+'"')) -WindowStyle Hidden -PassThru
    try {
        Wait-Until {$process.Refresh();$process.MainWindowHandle -ne 0} "No window for $file"
        Wait-Until { $names=Names $process; ($names -contains '固定置顶') -and -not ($names -contains '正在加载…') } "Load timed out: $file"
        Start-Sleep -Milliseconds 300
        $process.Refresh()
        if($process.MainWindowTitle -ne '') { throw 'Preview caption should be empty' }
        if((Names $process) -contains '打开文件') { throw 'Top toolbar is still visible' }
        $rect=[PeekNativeTest+RECT]::new()
        [void][PeekNativeTest]::GetWindowRect($process.MainWindowHandle,[ref]$rect)
        $monitor=[PeekNativeTest+MONITORINFO]::new();$monitor.Size=40
        [void][PeekNativeTest]::GetMonitorInfo([PeekNativeTest]::MonitorFromWindow($process.MainWindowHandle,2),[ref]$monitor)
        if([Math]::Abs(($rect.Left+$rect.Right)-($monitor.Work.Left+$monitor.Work.Right)) -gt 2 -or [Math]::Abs(($rect.Top+$rect.Bottom)-($monitor.Work.Top+$monitor.Work.Bottom)) -gt 2) {throw "Preview is not centered: window=$($rect.Left),$($rect.Top),$($rect.Right),$($rect.Bottom) work=$($monitor.Work.Left),$($monitor.Work.Top),$($monitor.Work.Right),$($monitor.Work.Bottom)"}
        if(([PeekNativeTest]::GetWindowLong($process.MainWindowHandle,-20) -band 8) -ne 0) {throw 'Preview should not default to topmost'}
        Invoke-Control $process '固定置顶'
        Wait-Until {([PeekNativeTest]::GetWindowLong($process.MainWindowHandle,-20) -band 8) -ne 0} 'Pin did not enable topmost'
        Invoke-Control $process '取消置顶'
        Wait-Until {([PeekNativeTest]::GetWindowLong($process.MainWindowHandle,-20) -band 8) -eq 0} 'Unpin did not disable topmost'
        if(((Names $process) -join '|') -match 'Space / Esc') {throw 'Footer hints are still visible'}
        if((Names $process) -contains '更多操作') {throw 'More button is still visible'}
        if(-not ((Names $process) -contains $file)) {throw 'Filename missing from header'}
        if((Names $process) -contains '在文档中搜索') {throw 'Search should be hidden initially'}
        $root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $pinName=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'固定置顶')
        $minName=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Minimize')
        $pinBounds=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$pinName).Current.BoundingRectangle
        $minBounds=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$minName).Current.BoundingRectangle
        if([Math]::Abs($pinBounds.Top-$minBounds.Top) -gt 2 -or [Math]::Abs($pinBounds.Height-$minBounds.Height) -gt 2 -or [Math]::Abs($pinBounds.Right-$minBounds.Left) -gt 2) {throw "Caption pin misaligned: $pinBounds / $minBounds"}
        # PDF now uses the native WebView2 reader rather than a raster-page label.
        # Real wheel/zoom/drag-copy coverage is in test-reading.ps1.
        # Reader search and syntax colors are verified by test-reader.cjs.
        if($file -eq 'sample.mp4') {
            # Opening media starts playback automatically.
            Wait-Until { ((Names $process) -join '|') -match '00:0[1-7] / 00:08' } 'Video time did not advance'
            Invoke-Control $process '暂停'
            Invoke-Control $process '静音'
            Wait-Until {(Names $process) -contains '取消静音'} 'Mute state missing'
            Invoke-Control $process '取消静音'
            Invoke-Control $process '播放速度'
            Wait-Until {(Names $process) -contains '1.5×'} 'Playback rate did not change'
            $root=[System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
            $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'播放进度')
            $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition).GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue(4.0)
            Wait-Until {((Names $process) -join '|') -match '00:04 / 00:08'} 'Seek failed'
        }
        # WebView content, source toggles and search are checked by test-reader.cjs.
        $results += "$file : PASS"
    } finally { if(-not $process.HasExited){[void]$process.CloseMainWindow();[void]$process.WaitForExit(3000)} }
}
if($UiOnly) {
    $results | Set-Content 'docs/evidence/ui-tests.txt' -Encoding UTF8
    $results
    return
}
# Integration with a dedicated Explorer window. No user files are edited.
$broker=Start-Process -FilePath $exePath -WindowStyle Hidden -PassThru -RedirectStandardError target/broker-native.log
try {
    Start-Process explorer.exe -ArgumentList ('/select,'+[IO.Path]::GetFullPath('tests/fixtures/example.json'))
    $shell=New-Object -ComObject Shell.Application
    $uri=([Uri][IO.Path]::GetFullPath('tests/fixtures')).AbsoluteUri
    Wait-Until {$script:explorer=$shell.Windows()|Where-Object {$_.LocationURL.TrimEnd('/') -eq $uri.TrimEnd('/')}|Select-Object -Last 1; $null -ne $script:explorer} 'Explorer not ready'
    $script:explorer.Document.SelectItem($script:explorer.Document.Folder.ParseName('example.json'),29)
    Activate ([IntPtr]$script:explorer.HWND)
    Key 32
    Wait-Until {$script:preview=Get-Process quickpeek -ErrorAction SilentlyContinue|Where-Object {$_.Id -ne $broker.Id -and $_.MainWindowHandle -ne 0}|Select-Object -First 1;$null -ne $script:preview} 'Explorer Space did not open JSON'
    Wait-Until {$script:preview.Refresh(); $script:preview.MainWindowTitle -eq 'example.json'} 'Default GPUI preview missing'
    $gpuiProcess = $script:preview
    Key 13
    Wait-Until {$gpuiProcess.Refresh(); $gpuiProcess.HasExited} 'GPUI did not hand off to the classic reader'
    Wait-Until {
        $script:preview = Get-Process quickpeek -ErrorAction SilentlyContinue | Where-Object {$_.Id -ne $broker.Id -and $_.MainWindowHandle -ne 0 -and (Names $_) -contains 'example.json'} | Select-Object -First 1
        $null -ne $script:preview
    } 'Classic JSON reader missing'
    Start-Sleep -Milliseconds 600
    Key 39
    Wait-Until {(Names $script:preview) -contains 'example.rs'} 'Arrow navigation failed'
    Key 32
    Wait-Until {$script:preview.Refresh();$script:preview.HasExited} 'Space did not close preview'
    $broker.Refresh();if($broker.HasExited){throw 'Broker exited with preview'}
    $results += 'Explorer Space / Right / Space / resident broker : PASS'
    Activate ([IntPtr]$script:explorer.HWND)
    $script:explorer.Document.SelectItem($script:explorer.Document.Folder.ParseName('example.json'),29)
    Start-Sleep -Milliseconds 300
    $renameCondition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'example.json')
    Wait-Until {
        $root=[System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$script:explorer.HWND)
        $script:renameItem=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$renameCondition)
        $null -ne $script:renameItem
    } 'Explorer item accessibility tree not ready'
    $script:renameItem.SetFocus()
    Key 113 # F2; cancel without saving.
    Start-Sleep -Milliseconds 300
    $focus=[System.Windows.Automation.AutomationElement]::FocusedElement
    Wait-Until {$f=[System.Windows.Automation.AutomationElement]::FocusedElement; $f.Current.ClassName -eq 'UIRenameTextElement' -or $f.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit} 'Rename field did not receive focus'
    Key 32
    Key 27
    Start-Sleep -Milliseconds 300
    if(Get-Process quickpeek -ErrorAction SilentlyContinue|Where-Object {$_.Id -ne $broker.Id -and $_.MainWindowHandle -ne 0}) {throw 'Space intercepted rename input'}
    $results += 'Explorer rename input is not intercepted : PASS'
} catch {
    Write-Output "Primary failure: $_"
    throw
} finally {
    $hwnd=[PeekNativeTest]::FindWindow('QuickPeek.Broker.v1','QuickPeek')
    if($hwnd -ne [IntPtr]::Zero){[void][PeekNativeTest]::PostMessage($hwnd,16,[IntPtr]::Zero,[IntPtr]::Zero)}
    if(-not $broker.WaitForExit(3000)){throw 'Broker failed to exit cleanly'}
}
$results += 'Tray broker shutdown : PASS'
$resultFile=if($KeyboardOnly){"docs/evidence/keyboard-tests.txt"}else{"docs/evidence/native-tests.txt"}
$results | Set-Content $resultFile -Encoding UTF8
$results

