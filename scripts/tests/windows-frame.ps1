param([Parameter(Mandatory = $true)][string]$State, [switch]$InspectOnly, [switch]$Exercise)
# -Exercise also moves, resizes, minimises and restores each session window and simulates a monitor
# DPI change, and checks the layout right after each step and once it settles. It acts only on a
# window that lies wholly off every monitor, and never activates a window or moves the cursor.

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
    [DllImport("user32.dll")] static extern long GetWindowLongPtr(IntPtr hwnd, int index);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int w, int h, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualAllocEx(IntPtr process, IntPtr address, IntPtr size, uint type, uint protect);
    [DllImport("kernel32.dll")] static extern bool VirtualFreeEx(IntPtr process, IntPtr address, IntPtr size, uint type);
    [DllImport("kernel32.dll")] static extern bool WriteProcessMemory(IntPtr process, IntPtr address, int[] data, IntPtr size, out IntPtr written);
    public static Rect WindowRect(long handle) { Rect r; GetWindowRect(new IntPtr(handle), out r); return r; }
    public static uint ProcessOf(long handle) { uint pid; GetWindowThreadProcessId(new IntPtr(handle), out pid); return pid; }
    // WM_DPICHANGED carries a pointer to the suggested window rectangle, which the browser reads in
    // its own process, so the rectangle is written into that process for the one synchronous send.
    public static bool SimulateDpi(long handle, uint dpi, Rect suggested) {
        var hwnd=new IntPtr(handle); uint pid; GetWindowThreadProcessId(hwnd,out pid);
        var process=OpenProcess(0x0008|0x0020|0x0400,false,pid);
        if(process==IntPtr.Zero) return false;
        try {
            var memory=VirtualAllocEx(process,IntPtr.Zero,new IntPtr(16),0x3000,0x04);
            if(memory==IntPtr.Zero) return false;
            try {
                IntPtr written;
                if(!WriteProcessMemory(process,memory,new[]{suggested.Left,suggested.Top,suggested.Right,suggested.Bottom},new IntPtr(16),out written)) return false;
                IntPtr result;
                var w=new IntPtr((int)((dpi<<16)|dpi));
                return SendMessageTimeout(hwnd,0x02E0,w,memory,0x0002,5000,out result)!=IntPtr.Zero;
            } finally { VirtualFreeEx(process,memory,IntPtr.Zero,0x8000); }
        } finally { CloseHandle(process); }
    }
    static string Class(IntPtr hwnd) { var text=new StringBuilder(256); GetClassName(hwnd,text,256); return text.ToString(); }
    public class Observation {
        public long Browser; public uint Dpi; public bool RivetWindowExists, BrowserIsTopLevel, BrowserIsResizable;
        public int ClientLeft, ClientRight, ContentLeft, ContentRight, ExpectedSidebar;
    }
    // `dpi` is the scale the runtime traced for the window, or the DPI a simulated WM_DPICHANGED
    // told the browser.
    public static Observation Read(long handle, uint dpi) {
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
            return new Observation {
                Browser=handle, Dpi=dpi, RivetWindowExists=rivet,
                BrowserIsTopLevel=GetParent(browser)==IntPtr.Zero && (style&0x40000000)==0,
                BrowserIsResizable=(style&0x00040000)!=0,
                ClientLeft=origin.X, ClientRight=origin.X+c.Right, ContentLeft=r.Left, ContentRight=r.Right,
                ExpectedSidebar=(int)((276*dpi+48)/96)
            };
        } finally { SetThreadDpiAwarenessContext(prior); }
    }
}
'@
# The scale the runtime traced for the window: the measurement uses the browser's own scale, never
# this process's idea of the window's DPI. The runtime traces only when the session's profile has a
# `rivet-debug` folder, which the profile is found from on the browser's command line.
function Get-TracedDpi([long]$Handle) {
    $process = Get-CimInstance Win32_Process -Filter "ProcessId = $([RivetFrameCheck]::ProcessOf($Handle))"
    if (-not $process -or $process.CommandLine -notmatch '--user-data-dir=(?:"([^"]+)"|(\S+))') { throw 'FR-2: The browser window has no profile on its command line.' }
    $profile = if ($Matches[1]) { $Matches[1] } else { $Matches[2] }
    $log = Join-Path $profile 'rivet-debug\rivet-vertical-tabs.log'
    $hex = '{0:x}' -f $Handle
    $lines = if (Test-Path -LiteralPath $log) { @(Get-Content -LiteralPath $log | Where-Object { $_ -match '^layout dpi: window 0x([0-9a-f]+) at (\d+)$' }) } else { @() }
    $line = @($lines | Where-Object { $_ -match "^layout dpi: window 0x$hex at" }) | Select-Object -Last 1
    if (-not $line) { $line = $lines | Select-Object -Last 1 }
    if (-not $line -or $line -notmatch ' at (\d+)$') { throw "FR-2: The runtime traced no scale for the window; start the session with a rivet-debug folder in $profile." }
    [uint32]$Matches[1]
}

# The checks of one observation. `Step` names what was just done to the window, for the message.
function Test-Layout($Item, [string]$Step) {
    $at = if ($Step) { " ($Step)" } else { '' }
    if ($Item.RivetWindowExists) { throw "FR-2: Rivet created a window of its own beside the browser$at." }
    if (!$Item.BrowserIsTopLevel) { throw "FR-2: The browser window was reparented into another window$at." }
    if (!$Item.BrowserIsResizable) { throw "FR-2: The browser window is fullscreen or lost its native frame$at." }
    # Fractional monitor scales round the browser's child window by up to one pixel.
    $left = $Item.ContentLeft - $Item.ClientLeft
    if ([Math]::Abs($left - $Item.ExpectedSidebar) -gt 1) { throw "FR-2: The browser content starts $left px from the client edge, not beside the $($Item.ExpectedSidebar) px sidebar and its 8 DIP panel inset$at." }
    # The page keeps the frame's 8 DIP gap to the window edge.
    $gap = [int][Math]::Floor((8 * $Item.Dpi + 48) / 96)
    $right = $Item.ClientRight - $Item.ContentRight
    if ([Math]::Abs($right - $gap) -gt 1) { throw "FR-2: The browser content ends $right px from the window edge, not one $gap px gap$at." }
}

# Whether a screen rectangle touches any monitor. A window that does is the owner's to see, so the
# exercise never moves it.
function Test-OnAMonitor($Rect) {
    Add-Type -AssemblyName System.Windows.Forms
    foreach ($screen in [System.Windows.Forms.Screen]::AllScreens) {
        $b = $screen.Bounds
        if ($Rect.Left -lt $b.Right -and $b.Left -lt $Rect.Right -and $Rect.Top -lt $b.Bottom -and $b.Top -lt $Rect.Bottom) { return $true }
    }
    $false
}

# Checks the layout right after a step, while the browser may still be laying out, and again once it
# settles. The first look comes within a few frames of the step.
function Test-Step([long]$Handle, [string]$Step, [uint32]$Dpi = 0) {
    Start-Sleep -Milliseconds 40
    if (-not $Dpi) { $Dpi = Get-TracedDpi $Handle }
    $during = [RivetFrameCheck]::Read($Handle, $Dpi)
    Start-Sleep -Milliseconds 800
    $after = [RivetFrameCheck]::Read($Handle, $Dpi)
    Test-Layout $during "during $Step"
    Test-Layout $after "after $Step"
    Write-Output "NFR-5 $Step`: content $($after.ContentLeft - $after.ClientLeft) px from the client edge at $($after.Dpi) DPI"
}

function Invoke-Exercise([long]$Handle) {
    $start = [RivetFrameCheck]::WindowRect($Handle)
    if (Test-OnAMonitor $start) { throw 'NFR-5: The session window lies on a monitor; the exercise moves only off-screen windows.' }
    $w = $start.Right - $start.Left; $h = $start.Bottom - $start.Top
    $hwnd = [IntPtr]$Handle
    # SWP_NOZORDER | SWP_NOACTIVATE. Each place is further from the monitors than the start.
    $flags = 0x0014
    $x = $start.Left + 400; $y = $start.Top + 200
    try {
        [RivetFrameCheck]::SetWindowPos($hwnd, [IntPtr]::Zero, $x, $y, $w, $h, $flags) | Out-Null
        Test-Step $Handle 'move'
        [RivetFrameCheck]::SetWindowPos($hwnd, [IntPtr]::Zero, $x, $y, [int]($w * 0.7), [int]($h * 0.8), $flags) | Out-Null
        Test-Step $Handle 'resize smaller'
        [RivetFrameCheck]::SetWindowPos($hwnd, [IntPtr]::Zero, $x, $y, $w + 300, $h + 120, $flags) | Out-Null
        Test-Step $Handle 'resize larger'
        # SW_SHOWMINNOACTIVE, then SW_SHOWNOACTIVATE puts it back in place without activating it.
        [RivetFrameCheck]::ShowWindow($hwnd, 7) | Out-Null
        Start-Sleep -Milliseconds 600
        if (-not [RivetFrameCheck]::IsIconic($hwnd)) { throw 'NFR-5: The window did not minimise.' }
        [RivetFrameCheck]::ShowWindow($hwnd, 4) | Out-Null
        Test-Step $Handle 'restore'
        # A monitor DPI change, simulated, since moving onto another monitor would show the window.
        # The browser lays itself out for the DPI it is told, so the check expects that DPI's sidebar
        # and gap. The window then gets its own DPI back.
        $own = Get-TracedDpi $Handle
        $other = if ($own -ge 144) { [uint32]96 } else { [uint32]144 }
        $now = [RivetFrameCheck]::WindowRect($Handle)
        $suggested = New-Object RivetFrameCheck+Rect
        $suggested.Left = $now.Left; $suggested.Top = $now.Top
        $suggested.Right = $now.Left + [int](($now.Right - $now.Left) * $other / $own)
        $suggested.Bottom = $now.Top + [int](($now.Bottom - $now.Top) * $other / $own)
        if (-not [RivetFrameCheck]::SimulateDpi($Handle, $other, $suggested)) { throw 'NFR-5: The simulated DPI change could not be sent.' }
        Test-Step $Handle "dpi $own to $other" $other
        if (-not [RivetFrameCheck]::SimulateDpi($Handle, $own, $now)) { throw 'NFR-5: The DPI could not be set back.' }
        Test-Step $Handle "dpi $other to $own" $own
        Write-Output 'NFR-5 maximise and snap: NOT_DRIVEN, both put the window on a visible monitor'
    } finally {
        [RivetFrameCheck]::SetWindowPos($hwnd, [IntPtr]::Zero, $start.Left, $start.Top, $w, $h, $flags) | Out-Null
    }
}

$sessions = (Get-Content -LiteralPath $State | ConvertFrom-Json).sessions.PSObject.Properties.Value
if (@($sessions).Count -eq 0) { throw 'FR-2: The state file has no Rivet session.' }
foreach ($session in $sessions) {
    $item = [RivetFrameCheck]::Read($session.window_handle, (Get-TracedDpi $session.window_handle))
    $item | ConvertTo-Json
    if ($InspectOnly) { continue }
    Test-Layout $item ''
    if ($Exercise) { Invoke-Exercise $session.window_handle }
}
if (!$InspectOnly) { Write-Output 'FR-2 native frame geometry: PASS' }
if ($Exercise -and !$InspectOnly) { Write-Output 'NFR-5 layout through move, resize, restore and DPI change: PASS' }
