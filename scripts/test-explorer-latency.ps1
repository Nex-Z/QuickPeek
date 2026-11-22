param(
    [string]$Report = 'target/explorer-latency.csv',
    [ValidateRange(1,1000)][int]$Rounds = 100,
    [switch]$ImagesOnly
)
Set-Location (Split-Path $PSScriptRoot -Parent)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
if (-not ("QuickPeekLatency" -as [type])) {
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class QuickPeekLatency {
 [DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")]public static extern void keybd_event(byte key,byte s,uint f,UIntPtr e);
 [DllImport("user32.dll")]public static extern int GetWindowText(IntPtr h,System.Text.StringBuilder s,int n);
 [DllImport("user32.dll")]public static extern IntPtr GetDC(IntPtr h);
 [DllImport("user32.dll")]public static extern int ReleaseDC(IntPtr h,IntPtr dc);
 [DllImport("gdi32.dll")]public static extern uint GetPixel(IntPtr dc,int x,int y);
 [DllImport("user32.dll")]public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr v);
 [DllImport("user32.dll")]public static extern bool GetClientRect(IntPtr h,out R r);
 [StructLayout(LayoutKind.Sequential)]public struct R {public int l,t,r,b;}
 public static void Press(byte k){keybd_event(k,0,0,UIntPtr.Zero);keybd_event(k,0,2,UIntPtr.Zero);}
 [StructLayout(LayoutKind.Sequential)] public struct P{public int x,y;}
 [DllImport("user32.dll")]public static extern bool ClientToScreen(IntPtr h,ref P p);
 [DllImport("user32.dll")]public static extern uint GetDpiForWindow(IntPtr hwnd);
 [DllImport("gdi32.dll")]static extern IntPtr CreateCompatibleDC(IntPtr dc);
 [DllImport("gdi32.dll")]static extern IntPtr CreateCompatibleBitmap(IntPtr dc,int w,int h);
 [DllImport("gdi32.dll")]static extern IntPtr SelectObject(IntPtr dc,IntPtr obj);
 [DllImport("gdi32.dll")]static extern bool BitBlt(IntPtr dst,int x,int y,int w,int h,IntPtr src,int sx,int sy,uint mode);
 [DllImport("gdi32.dll")]static extern bool DeleteDC(IntPtr dc);
 [DllImport("gdi32.dll")]static extern bool DeleteObject(IntPtr obj);
 // Text samples put dark glyphs in opposite rows to distinguish them from
 // an empty frame or the previous document. Read from one small screen copy.
 static int Ink(IntPtr buffer, double scale, int row) {
     int count = 0;
     for (int y = 5; y < 21; y += 3) {
         for (int x = 18; x < 100; x += 4) {
             uint color = GetPixel(buffer, (int)(x*scale), (int)((32+row*23+y)*scale));
             if ((color & 255) < 140 && ((color >> 8) & 255) < 140 && ((color >> 16) & 255) < 140)
                 count++;
         }
     }
     return count;
 }
 [DllImport("user32.dll")]public static extern bool IsWindowVisible(IntPtr hwnd);
 public static bool Probe(string title, int expected, out bool window) {
     var hwnd = GetForegroundWindow();
     var caption = new System.Text.StringBuilder(256);
     GetWindowText(hwnd,caption,caption.Capacity);
     window = caption.ToString() == title;
     if (!window) return false;
     var screen = GetDC(IntPtr.Zero);
     try {
         if (expected >= 0) {
             R rect;
             GetClientRect(hwnd, out rect);
             var center = new P{x=rect.r/2, y=rect.b/2};
             ClientToScreen(hwnd,ref center);
             return GetPixel(screen,center.x,center.y) == expected;
         }
         var origin = new P();
         ClientToScreen(hwnd,ref origin);
         double scale = GetDpiForWindow(hwnd)/96.0;
         int width = (int)(110*scale), height = (int)(90*scale);
         var buffer = CreateCompatibleDC(screen);
         var bitmap = CreateCompatibleBitmap(screen,width,height);
         var previous = SelectObject(buffer,bitmap);
         try {
             if (!BitBlt(buffer,0,0,width,height,screen,origin.x,origin.y,0x00CC0020)) return false;
             int row = expected == -1 ? 0 : 1;
             return Ink(buffer,scale,row)>3 && Ink(buffer,scale,1-row)==0;
         } finally {
             SelectObject(buffer,previous);
             DeleteObject(bitmap);
             DeleteDC(buffer);
         }
     } finally {
         ReleaseDC(IntPtr.Zero,screen);
     }
 }

}
"@
}
[void][QuickPeekLatency]::SetThreadDpiAwarenessContext([IntPtr](-4))
$dir = Join-Path ([IO.Path]::GetFullPath('target')) ('explorer-latency-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($dir)
foreach ($sample in @(@('amber.png',236,121,33),@('green.png',36,190,112))) {
    $image = [Drawing.Bitmap]::new(320,180)
    $graphics = [Drawing.Graphics]::FromImage($image)
    try {
        $graphics.Clear([Drawing.Color]::FromArgb($sample[1],$sample[2],$sample[3]))
        $image.Save((Join-Path $dir $sample[0]),[Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $image.Dispose() }
}
[IO.File]::WriteAllText((Join-Path $dir 'text.txt'),"MMMMMMMMMMMMMMMMMMMMMMMMMMMMM`n`nPreview`n")
[IO.File]::WriteAllText((Join-Path $dir 'code.ps1'),"`n# MMMMMMMMMMMMMMMMMMMMMMMMMMMMM`n`n")
$names = @('amber.png','text.txt','green.png','code.ps1')
$colors = @(0x2179ec,-1,0x70be24,-2)
if ($ImagesOnly) { $names=@('amber.png','green.png'); $colors=@(0x2179ec,0x70be24) }
$shell=New-Object -ComObject Shell.Application
$existing=@($shell.Windows()|ForEach-Object HWND)
Start-Process explorer.exe -ArgumentList ('/select,"'+$dir+'\amber.png"')
try{
 for($i=0;$i -lt 80;$i++){ $expl=$shell.Windows()|Where-Object {$_.Document.Folder.Self.Path -eq $dir}|Select-Object -Last 1;if($expl){break};Start-Sleep -Milliseconds 80}
 $h=[IntPtr]$expl.HWND
 $root=[System.Windows.Automation.AutomationElement]::FromHandle($h)
 $type=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::ListItem)
 for($j=0;$j -lt 60;$j++){
  $items=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$type)
  if(@($items | Where-Object {$_.Current.Name -in $names}).Count -eq $names.Count){break}
  Start-Sleep -Milliseconds 50
 }
 $results=foreach($i in 0..($Rounds-1)){
  $title=$names[$i%$names.Count]; $color=$colors[$i%$names.Count]
  if([QuickPeekLatency]::GetForegroundWindow() -ne $h){[QuickPeekLatency]::Press(18);[void][QuickPeekLatency]::SetForegroundWindow($h)}
  $expl.Document.SelectItem($expl.Document.Folder.ParseName($title),29)
  ($items|Where-Object {$_.Current.Name -eq $title}|Select-Object -First 1).SetFocus()
  if($i -eq 0){Start-Sleep -Milliseconds 300}
  if([QuickPeekLatency]::GetForegroundWindow() -ne $h){throw 'Explorer focus lost'}
  $sw=[Diagnostics.Stopwatch]::StartNew();[QuickPeekLatency]::Press(32);$windowMs=$null;$body=$null
  do{$visible=$false;$hit=[QuickPeekLatency]::Probe($title,$color,[ref]$visible);if($visible -and $null -eq $windowMs){$windowMs=$sw.ElapsedMilliseconds};if($hit){$body=$sw.ElapsedMilliseconds;break};Start-Sleep -Milliseconds 10}while($sw.ElapsedMilliseconds -lt 2200)
  $closed=$false
  if($visible){
   $preview=[QuickPeekLatency]::GetForegroundWindow()
   [QuickPeekLatency]::Press(32)
   for($j=0;$j -lt 60;$j++){
    if(![QuickPeekLatency]::IsWindowVisible($preview) -and [QuickPeekLatency]::GetForegroundWindow() -eq $h){$closed=$true;break}
    Start-Sleep -Milliseconds 10
   }
  }
  [pscustomobject]@{round=$i+1;name=$title;window_ms=$windowMs;body_ms=$body;closed=$closed}
 }
 $results|Export-Csv $Report -NoTypeInformation
 $results|Format-Table
 if(@($results|Where-Object {$null -eq $_.body_ms -or !$_.closed}).Count){throw "Preview or close failed. See $Report"}
} finally {
 if($expl -and $expl.HWND -notin $existing){$expl.Quit()}
 foreach($name in 'amber.png','green.png','text.txt','code.ps1'){[IO.File]::Delete((Join-Path $dir $name))}
 [IO.Directory]::Delete($dir)
}
