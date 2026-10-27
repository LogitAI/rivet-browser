param([Parameter(Mandatory = $true)][string]$State, [switch]$InspectOnly)

$ErrorActionPreference = 'Stop'
# Reads native window geometry of the Rivet sessions in a state file, without moving windows or
# reading page content. The sidebar is a composition visual of the browser window, so the check
# looks at where the browser placed its own web content.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class RivetFrameCheck {
    public delegate bool Visit(IntPtr hwnd, IntPtr data);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] static extern bool EnumWindows(Visit callback, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr hwnd, Visit callback, IntPtr data);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr hwnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern long GetWindowLongPtr(IntPtr hwnd, int index);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    static string Class(IntPtr hwnd) { var text=new StringBuilder(256); GetClassName(hwnd,text,256); return text.ToString(); }
    public class Observation {
        public long Browser; public uint Dpi; public bool RivetWindowExists, BrowserIsTopLevel, BrowserIsResizable;
        public int ClientLeft, ClientRight, ContentLeft, ContentRight, ExpectedSidebar;
    }
    public static Observation Read(long handle) {
        var prior=SetThreadDpiAwarenessContext(new IntPtr(-4));
        try {
            var browser=new IntPtr(handle);
            bool rivet=false;
            EnumWindows((window,p)=> { if(Class(window).StartsWith("Rivet") && IsWindowVisible(window)) rivet=true; return true; },IntPtr.Zero);
            IntPtr content=IntPtr.Zero;
            EnumChildWindows(browser,(child,q)=> {
                if(Class(child)=="Chrome_RenderWidgetHostHWND" && IsWindowVisible(child)) content=child;
                return true;
            },IntPtr.Zero);
            Rect c; GetClientRect(browser,out c);
            var origin=new Point(); ClientToScreen(browser,ref origin);
            var r=new Rect(); if(content!=IntPtr.Zero) GetWindowRect(content,out r);
            long style=GetWindowLongPtr(browser,-16);
            uint dpi=GetDpiForWindow(browser);
            return new Observation {
                Browser=handle, Dpi=dpi, RivetWindowExists=rivet,
                BrowserIsTopLevel=GetParent(browser)==IntPtr.Zero && (style&0x40000000)==0,
                BrowserIsResizable=(style&0x00040000)!=0,
                ClientLeft=origin.X, ClientRight=origin.X+c.Right, ContentLeft=r.Left, ContentRight=r.Right,
                ExpectedSidebar=(int)((268*dpi+48)/96)
            };
        } finally { SetThreadDpiAwarenessContext(prior); }
    }
}
'@
$sessions = (Get-Content -LiteralPath $State | ConvertFrom-Json).sessions.PSObject.Properties.Value
if (@($sessions).Count -eq 0) { throw 'FR-2: The state file has no Rivet session.' }
foreach ($session in $sessions) {
    $item = [RivetFrameCheck]::Read($session.window_handle)
    $item | ConvertTo-Json
    if ($InspectOnly) { continue }
    if ($item.RivetWindowExists) { throw 'FR-2: Rivet created a window of its own beside the browser.' }
    if (!$item.BrowserIsTopLevel) { throw 'FR-2: The browser window was reparented into another window.' }
    if (!$item.BrowserIsResizable) { throw 'FR-2: The browser window is fullscreen or lost its native frame.' }
    # Fractional monitor scales round the browser's child window by up to one pixel.
    if ([Math]::Abs($item.ContentLeft - $item.ClientLeft - $item.ExpectedSidebar) -gt 1) { throw 'FR-2: The browser did not lay its content out beside the sidebar.' }
    if ([Math]::Abs($item.ContentRight - $item.ClientRight) -gt 1) { throw 'FR-2: The browser content does not reach the window edge.' }
}
if (!$InspectOnly) { Write-Output 'FR-2 native frame geometry: PASS' }
