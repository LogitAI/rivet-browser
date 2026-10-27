param([switch]$InspectOnly, [switch]$ExpectWindowed)

$ErrorActionPreference = 'Stop'
# Read native window geometry without moving windows or accessing page content.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class RivetFrameCheck {
    public delegate bool Visit(IntPtr hwnd, IntPtr data);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo { public uint Size; public Rect Monitor, Work; public uint Flags; }
    [DllImport("user32.dll")] static extern bool EnumWindows(Visit callback, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr hwnd, Visit callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder text, int size);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr hwnd, uint key, out Rect value, uint size);
    static string Class(IntPtr hwnd) { var text=new StringBuilder(256); GetClassName(hwnd,text,256); return text.ToString(); }
    public class Observation {
        public long Host, Browser, Renderer; public uint ProcessId, Dpi;
        public int HostClientTop, HostClientLeft, BrowserWidth, BrowserHeight;
        public int RendererInsetLeft, RendererInsetTop, RendererInsetRight, RendererInsetBottom;
        public int CaptionButtonsWidth; public bool CaptionButtonsExposed, BrowserInsideHost;
        public int HostWidth, HostHeight, WorkWidth, WorkHeight; public bool HostIsWindowed;
    }
    public static Observation[] Read() {
        var prior=SetThreadDpiAwarenessContext(new IntPtr(-4));
        try {
            var result=new List<Observation>();
            EnumWindows((host,p)=> {
                if(Class(host)!="RivetHostWindow" || !IsWindowVisible(host) || IsIconic(host)) return true;
                IntPtr browser=IntPtr.Zero, renderer=IntPtr.Zero;
                EnumChildWindows(host,(child,q)=> {
                    if(Class(child)=="Chrome_WidgetWin_1") browser=child;
                    if(Class(child)=="Chrome_RenderWidgetHostHWND") renderer=child;
                    return true;
                },IntPtr.Zero);
                if(browser==IntPtr.Zero || renderer==IntPtr.Zero) return true;
                GetWindowRect(host,out var h); GetClientRect(host,out var client);
                var monitor=new MonitorInfo { Size=(uint)Marshal.SizeOf(typeof(MonitorInfo)) };
                GetMonitorInfo(MonitorFromWindow(host,2),ref monitor);
                var origin=new Point(); ClientToScreen(host,ref origin);
                GetWindowRect(browser,out var b); GetWindowRect(renderer,out var r);
                DwmGetWindowAttribute(host,5,out var buttons,16);
                GetWindowThreadProcessId(host,out var pid);
                result.Add(new Observation {
                    Host=host.ToInt64(),Browser=browser.ToInt64(),Renderer=renderer.ToInt64(),ProcessId=pid,Dpi=GetDpiForWindow(host),
                    HostClientTop=origin.Y-h.Top,HostClientLeft=origin.X-h.Left,
                    BrowserWidth=b.Right-b.Left,BrowserHeight=b.Bottom-b.Top,
                    RendererInsetLeft=r.Left-b.Left,RendererInsetTop=r.Top-b.Top,
                    RendererInsetRight=b.Right-r.Right,RendererInsetBottom=b.Bottom-r.Bottom,
                    CaptionButtonsWidth=buttons.Right-buttons.Left,
                    CaptionButtonsExposed=buttons.Bottom<=b.Top-h.Top && buttons.Right>buttons.Left,
                    BrowserInsideHost=b.Left>=origin.X && b.Top>=origin.Y && b.Right<=origin.X+client.Right && b.Bottom<=origin.Y+client.Bottom,
                    HostWidth=h.Right-h.Left,HostHeight=h.Bottom-h.Top,
                    WorkWidth=monitor.Work.Right-monitor.Work.Left,WorkHeight=monitor.Work.Bottom-monitor.Work.Top,
                    HostIsWindowed=h.Left>monitor.Work.Left && h.Top>monitor.Work.Top && h.Right<monitor.Work.Right && h.Bottom<monitor.Work.Bottom
                });
                return true;
            },IntPtr.Zero);
            return result.ToArray();
        } finally { SetThreadDpiAwarenessContext(prior); }
    }
}
'@
$observations = @([RivetFrameCheck]::Read())
$observations | ConvertTo-Json -Depth 4
if ($InspectOnly) { return }
if ($observations.Count -eq 0) { throw 'FR-2: No visible Rivet host with a web renderer.' }
foreach ($item in $observations) {
    if ($item.HostClientTop -ne 0 -or $item.HostClientLeft -ne 0) { throw 'FR-2: Rivet client does not extend to the window edges.' }
    if ($item.RendererInsetLeft -ne 0 -or $item.RendererInsetTop -ne 0 -or $item.RendererInsetRight -ne 0 -or $item.RendererInsetBottom -ne 0) {
        throw 'FR-2: Chrome still reserves a title or border outside the web renderer.'
    }
    if (!$item.BrowserInsideHost) { throw 'FR-2: Chrome extends outside the Rivet client.' }
    if (!$item.CaptionButtonsExposed) { throw 'FR-2: Native caption buttons overlap browser content or are absent.' }
    if ($ExpectWindowed -and !$item.HostIsWindowed) { throw 'FR-2: The initial Rivet window fills the monitor work area.' }
}
Write-Output 'FR-2 native frame geometry: PASS'
