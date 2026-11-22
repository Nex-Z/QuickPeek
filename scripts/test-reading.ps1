param([string]$Exe = 'target/release/quickpeek.exe', [string]$EvidenceDirectory = 'docs/evidence')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ReadingInput {
 [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
 [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
 [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
 [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h,uint f);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h,System.Text.StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
 // Cursor warping alone does not deliver drag moves to WebView2. Send real
 // absolute mouse input across the virtual desktop, including negative origins.
 public static bool MovePointer(int x,int y) {
   int left=GetSystemMetrics(76),top=GetSystemMetrics(77),width=GetSystemMetrics(78),height=GetSystemMetrics(79);
   INPUT input=new INPUT {type=0,mi=new MI {dx=(int)((x-left)*65535L/(width-1)),dy=(int)((y-top)*65535L/(height-1)),flags=0xC001}};
   return SendInput(1,new INPUT[]{input},Marshal.SizeOf(typeof(INPUT)))==1;
 }
 [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);
 [StructLayout(LayoutKind.Sequential)] public struct MI { public int dx,dy; public uint data,flags,time; public UIntPtr extra; }
 [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MI mi; }
 [DllImport("user32.dll",SetLastError=true)] public static extern uint SendInput(uint n, INPUT[] inputs,int size);
 public static void mouse_event(uint f,uint x,uint y,int data,UIntPtr e) {
   INPUT input=new INPUT { type=0,mi=new MI {dx=(int)x,dy=(int)y,data=unchecked((uint)data),flags=f,extra=e} };
   if(SendInput(1,new INPUT[]{input},Marshal.SizeOf(typeof(INPUT)))!=1) throw new Exception("SendInput failed");
 }
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
}
'@
[void][ReadingInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
$exePath = [IO.Path]::GetFullPath($Exe)
$evidence = [IO.Path]::GetFullPath($EvidenceDirectory)
New-Item -ItemType Directory -Force $evidence | Out-Null
$fixtureDir = Join-Path ([IO.Path]::GetTempPath()) ('QuickPeek-reading-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $fixtureDir | Out-Null
(1..240 | ForEach-Object { "Selectable line $_" }) | Set-Content (Join-Path $fixtureDir 'long.txt')
$bitmap = [System.Drawing.Bitmap]::new(600,2400)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.Clear([System.Drawing.Color]::LightBlue)
$graphics.FillRectangle([System.Drawing.Brushes]::Navy,0,2000,600,400)
$bitmap.Save((Join-Path $fixtureDir 'long.png'))
$graphics.Dispose(); $bitmap.Dispose()
$pdfFixture = Join-Path $fixtureDir '预览 #100% &+.pdf'
Copy-Item -LiteralPath 'tests/fixtures/sample.pdf' -Destination $pdfFixture
function Wait-Until([scriptblock]$Condition, [string]$Message) {
    for ($i=0; $i -lt 100; $i++) { if (& $Condition) { return }; Start-Sleep -Milliseconds 100 }
    throw $Message
}
function Elements {
    $script:process.Refresh()
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($script:process.MainWindowHandle)
    $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
}
function Named([string]$Name) { Elements | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1 }
function Key([byte]$Code) {
    [ReadingInput]::keybd_event($Code,0,0,[UIntPtr]::Zero)
    [ReadingInput]::keybd_event($Code,0,2,[UIntPtr]::Zero)
}
function Hover($Element) {
    $r=$Element.Current.BoundingRectangle
    [void][ReadingInput]::MovePointer([int]($r.X + [Math]::Min($r.Width/2,150)),[int]($r.Y + [Math]::Min($r.Height/2,150)))
    Start-Sleep -Milliseconds 150
}
function Click($Element) {
    Hover $Element
    [ReadingInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    [ReadingInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
}
function Wheel([int]$Delta,[switch]$Ctrl) {
    $pt=[ReadingInput+POINT]::new(); [void][ReadingInput]::GetCursorPos([ref]$pt)
    if ([ReadingInput]::GetAncestor([ReadingInput]::WindowFromPoint($pt),2) -ne $script:process.MainWindowHandle) { throw 'Pointer is outside the test preview' }
    if ($Ctrl) { [ReadingInput]::keybd_event(17,0,0,[UIntPtr]::Zero) }
    try { [ReadingInput]::mouse_event(0x800,0,0,$Delta,[UIntPtr]::Zero); Start-Sleep -Milliseconds 350 }
    finally { if ($Ctrl) { [ReadingInput]::keybd_event(17,0,2,[UIntPtr]::Zero) } }
}
function Capture([string]$Name) {
    & "$PSScriptRoot/inspect-window.ps1" -ProcessId $script:process.Id -Output (Join-Path $evidence $Name) | Out-Null
}
$results=@()
# Continuous text selection/search is covered in the embedded WebView by test-reader.cjs.
foreach ($file in @((Join-Path $fixtureDir 'long.png'),$pdfFixture)) {
    $script:process=Start-Process -FilePath $exePath -ArgumentList @('--classic',('"'+$file+'"')) -WindowStyle Hidden -PassThru
    try {
        Wait-Until { $script:process.Refresh(); $script:process.MainWindowHandle -ne 0 } 'Preview window missing'
        Wait-Until { (Named '固定置顶') -and -not (Named '正在加载…') } 'Preview load timed out'
        (Named '固定置顶').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        if ([ReadingInput]::GetForegroundWindow() -ne $script:process.MainWindowHandle) {
            Key 18
            if (-not [ReadingInput]::SetForegroundWindow($script:process.MainWindowHandle)) { throw 'Cannot activate test preview' }
        }
        Start-Sleep -Milliseconds 300
        if ([ReadingInput]::GetForegroundWindow() -ne $script:process.MainWindowHandle) { throw 'Test preview is not foreground' }
        if ($file.EndsWith('.png')) {
            Wait-Until { (Elements | Where-Object { $_.Current.Name -like '图片 · *' }).Count -gt 0 } 'Image missing'
            $window = [System.Windows.Automation.AutomationElement]::FromHandle($script:process.MainWindowHandle)
            Hover $window; Wheel -Delta 7200 -Ctrl
            $scroll = Elements | Where-Object { $_.GetSupportedPatterns().ProgrammaticName -contains 'ScrollPatternIdentifiers.Pattern' } | Select-Object -First 1
            $sp=$scroll.GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern)
            if (-not $sp.Current.VerticallyScrollable) { throw 'Long image is not scrollable' }
            $sp.SetScrollPercent(-1,50)
            Wait-Until { $sp.Current.VerticalScrollPercent -gt 0 } 'Image scrollbar failed'
            $zoomedImage = (Elements | Where-Object { $_.Current.Name -like '图片 · *' } | Select-Object -First 1).Current.Name
            Hover $scroll; Wheel -Delta -120
            Wait-Until { (Elements | Where-Object { $_.Current.Name -like '图片 · *' -and $_.Current.Name -ne $zoomedImage }).Count -gt 0 } 'Image wheel zoom failed'
            Wheel -Delta 120 -Ctrl
            Wait-Until { Named $zoomedImage } 'Image Ctrl+wheel zoom failed'
            Wait-Until { $sp.Current.HorizontallyScrollable } 'Zoomed image cannot scroll horizontally'
            Wheel -Delta -7200 -Ctrl
            Wait-Until { (Elements | Where-Object { $_.Current.Name -like '图片 · *' -and $_.Current.Name -ne $zoomedImage }).Count -gt 0 } 'Image zoom out failed'
            Capture 'reading-image.png'
            $results += 'Long image: scrollbar, wheel and Ctrl+wheel zoom in/out PASS'
        } else {
            Start-Sleep -Seconds 3
            $window=[System.Windows.Automation.AutomationElement]::FromHandle($script:process.MainWindowHandle).Current.BoundingRectangle
            $x1=[int]($window.Left+$window.Width*0.176)
            $x2=[int]($window.Left+$window.Width*0.50)
            $y=[int]($window.Top+$window.Height*0.27)
            Capture 'reading-pdf-unselected.png'
            [void][ReadingInput]::MovePointer($x1,$y)
            Start-Sleep -Milliseconds 200
            [ReadingInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
            for($step=1;$step -le 15;$step++) {
                [void][ReadingInput]::MovePointer([int]($x1+($x2-$x1)*$step/15),$y)
                Start-Sleep -Milliseconds 40
            }
            [ReadingInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
            Start-Sleep -Milliseconds 300
            Capture 'reading-pdf-selection.png'
            $selected = foreach ($element in (Elements)) {
                $pattern = $null
                if ($element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern,[ref]$pattern)) {
                    foreach ($range in $pattern.GetSelection()) { $range.GetText(200) }
                }
            }
            if (($selected -join ' ') -notmatch 'QuickPeek - Page 1') {
                # Some Edge PDF versions expose the highlight without a UIA text range.
                $before = [Drawing.Bitmap]::new((Join-Path $evidence 'reading-pdf-unselected.png'))
                $after = [Drawing.Bitmap]::new((Join-Path $evidence 'reading-pdf-selection.png'))
                try {
                    $changed = 0
                    for ($x = [int]($after.Width*.18); $x -lt $after.Width*.50; $x += 4) {
                        for ($line = [int]($after.Height*.23); $line -lt $after.Height*.31; $line += 4) {
                            $old = $before.GetPixel($x,$line)
                            $new = $after.GetPixel($x,$line)
                            if ($old.R -gt 230 -and $new.R -lt 210 -and $new.B -gt $new.R+25) { $changed++ }
                        }
                    }
                    if ($changed -lt 50) { throw 'PDF selection highlight missing from the dragged text' }
                } finally { $before.Dispose(); $after.Dispose() }
            }
            [void][ReadingInput]::MovePointer([int]($window.Left+$window.Width/2),[int]($window.Top+$window.Height/2))
            Start-Sleep -Milliseconds 150
            Capture 'reading-pdf-before-zoom.png'
            Wheel -Delta 120 -Ctrl
            Capture 'reading-pdf-zoom.png'
            if((Get-FileHash (Join-Path $evidence 'reading-pdf-before-zoom.png')).Hash -eq (Get-FileHash (Join-Path $evidence 'reading-pdf-zoom.png')).Hash){throw 'PDF zoom did not change rendering'}
            Wheel -Delta -120 -Ctrl
            Wheel -Delta -2400
            Capture 'reading-pdf.png'
            Key 27
            Wait-Until { $script:process.Refresh(); $script:process.HasExited } 'Esc did not close the PDF preview'
            $results += 'PDF: real mouse-drag text selection and Ctrl+wheel rendering change PASS; scroll screenshot saved for visual verification'
        }
    } finally {
        if (-not $script:process.HasExited) { [void]$script:process.CloseMainWindow(); [void]$script:process.WaitForExit(5000) }
    }
}
$results | Set-Content (Join-Path $evidence 'reading-tests.txt') -Encoding utf8
$results




