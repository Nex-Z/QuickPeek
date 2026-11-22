param([int]$ProcessId, [string]$Output = 'docs/evidence/window.png', [string]$Invoke)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class PeekCapture {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
'@
$process = Get-Process -Id $ProcessId
$handle = $process.MainWindowHandle
$root = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
if ($Invoke) {
  $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Invoke)
  $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
  if (-not $element) { throw "Control not found: $Invoke" }
  $pattern = $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
  $pattern.Invoke()
  Start-Sleep -Milliseconds 500
}
$elements = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$elements | ForEach-Object { if ($_.Current.Name) { $_.Current.Name } } | Select-Object -First 80
$rect = [PeekCapture+RECT]::new()
[void][PeekCapture]::GetWindowRect($handle, [ref]$rect)
$bitmap = [System.Drawing.Bitmap]::new($rect.Right-$rect.Left, $rect.Bottom-$rect.Top)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$dc = $graphics.GetHdc()
[void][PeekCapture]::PrintWindow($handle, $dc, 2)
$graphics.ReleaseHdc($dc)
$bitmap.Save([IO.Path]::GetFullPath($Output), [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
