# Runs a browser test runner on a desktop of its own, so no test window can reach the person's
# screen, keyboard or mouse by construction.
#
#   . scripts/tests/windows-test-desktop.ps1
#   $code = Invoke-OnOwnDesktop $PSCommandPath $PSBoundParameters
#   if ($null -ne $code) { exit $code }
#
# Invoke-OnOwnDesktop creates the desktop rivet-test-<pid> in this process's window station and starts
# the same runner there with the same arguments. Everything the runner starts, `rivet session start`,
# the launcher and the browser, inherits that desktop. Windows never shows another desktop's windows
# on the input desktop, never gives one of them the foreground and never routes the mouse to them.
# The runner's own PrintWindow captures and posted messages work there as they do anywhere.
#
# Inside the runner RIVET_TEST_DESKTOP names the desktop, and Invoke-OnOwnDesktop returns $null, so
# the runner goes on. Start-RunSafety then starts the visible guard on the default desktop with
# -TestDesktop: there no window of a test process may appear at all, and none may take the
# foreground or the mouse.
#
# RIVET_TEST_SAME_DESKTOP=1 skips the desktop, for a person who wants to watch a run.

if (-not ('RivetTestDesktop' -as [type])) {
  Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public sealed class RivetDesktopProcess{
 [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
 [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr h,out uint code);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 IntPtr handle;public readonly int Id;
 public RivetDesktopProcess(IntPtr h,int id){handle=h;Id=id;}
 public bool WaitForExit(int ms){return handle==IntPtr.Zero||WaitForSingleObject(handle,(uint)ms)==0;}
 public bool HasExited{get{return WaitForExit(0);}}
 public int ExitCode{get{uint code=0;if(handle!=IntPtr.Zero)GetExitCodeProcess(handle,out code);return (int)code;}}
 public void Dispose(){if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
}
public static class RivetTestDesktop{
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct SI{public int cb;public string reserved,desktop,title;public int x,y,w,h,xChars,yChars,fill,flags;public short show,reserved2;public IntPtr reserved3,input,output,error;}
 [StructLayout(LayoutKind.Sequential)] struct PI{public IntPtr process,thread;public int id,threadId;}
 [DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern bool CreateProcessW(string app,System.Text.StringBuilder line,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string dir,ref SI si,out PI pi);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int which);
 [DllImport("user32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern IntPtr CreateDesktopW(string name,IntPtr device,IntPtr mode,uint flags,uint access,IntPtr security);
 [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
 [DllImport("user32.dll")] static extern IntPtr GetProcessWindowStation();
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetUserObjectInformationW(IntPtr o,int index,System.Text.StringBuilder name,int length,out int needed);
 // The name of this process's window station: WinSta0 on a person's session, another on a service.
 public static string Station(){var b=new System.Text.StringBuilder(256);int n;return GetUserObjectInformationW(GetProcessWindowStation(),2,b,512,out n)?b.ToString():"WinSta0";}
 public delegate bool EW(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EW f,IntPtr l);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h,uint cmd);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h,System.Text.StringBuilder s,int n);
 [StructLayout(LayoutKind.Sequential)] struct RC{public int L,T,R,B;}
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out RC r);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern IntPtr GetPropW(IntPtr h,string name);
 // The visible, unowned Chromium frame window of the processes `pids` on this thread's desktop that Rivet
 // took, else the largest one. Some browsers keep more than one frame window.
 // A fresh desktop gives every process an input indicator window of its own, which .NET can pick as
 // the main window, so the browser frame is found by its class.
 public static IntPtr BrowserWindow(int[] pids){
  var set=new System.Collections.Generic.HashSet<uint>();foreach(var x in pids)set.Add((uint)x);
  IntPtr best=IntPtr.Zero;long area=-1;
  EnumWindows((h,l)=>{uint p;GetWindowThreadProcessId(h,out p);if(!set.Contains(p)||!IsWindowVisible(h)||GetWindow(h,4)!=IntPtr.Zero)return true;
   var b=new System.Text.StringBuilder(64);GetClassNameW(h,b,64);if(b.ToString()!="Chrome_WidgetWin_1")return true;
   RC r;GetWindowRect(h,out r);long a=(long)(r.R-r.L)*(r.B-r.T);
   if(GetPropW(h,"Rivet.OriginalWindowProc")!=IntPtr.Zero)a+=1L<<40;
   if(a>area){area=a;best=h;}return true;},IntPtr.Zero);
  return best;
 }
 // GENERIC_ALL on a new desktop of the calling process's window station.
 public static IntPtr Create(string name){var d=CreateDesktopW(name,IntPtr.Zero,IntPtr.Zero,0,0x10000000,IntPtr.Zero);if(d==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();return d;}
 public static bool Close(IntPtr desktop){return CloseDesktop(desktop);}
 // Starts the whole command line `line` on `desktop` ("WinSta0\name"). With `shareOutput` the new
 // process writes to this process's standard output and error. CREATE_NO_WINDOW keeps a console
 // program from making a console window there.
 public static RivetDesktopProcess Start(string desktop,string line,bool shareOutput){
  var si=new SI();si.cb=Marshal.SizeOf(typeof(SI));si.desktop=desktop;
  if(shareOutput){si.flags=0x100;si.input=GetStdHandle(-10);si.output=GetStdHandle(-11);si.error=GetStdHandle(-12);}
  PI pi;
  if(!CreateProcessW(null,new System.Text.StringBuilder(line),IntPtr.Zero,IntPtr.Zero,shareOutput,0x08000000,IntPtr.Zero,null,ref si,out pi))throw new System.ComponentModel.Win32Exception();
  CloseHandle(pi.thread);
  return new RivetDesktopProcess(pi.process,pi.id);
 }
}
'@
}

# One argument quoted for a Windows command line.
function ConvertTo-CommandArgument([string]$Value) {
  if ($Value -and $Value -notmatch '[\s"]') { return $Value }
  '"' + ($Value -replace '(\\*)"', '$1$1\"' -replace '(\\+)$', '$1$1') + '"'
}

# The bound parameters of a script as the arguments of `pwsh -File`, which takes a list as one
# comma-joined string.
function ConvertTo-RunnerArguments($Bound) {
  $out = @()
  foreach ($key in $Bound.Keys) {
    $value = $Bound[$key]
    if ($value -is [System.Management.Automation.SwitchParameter]) {
      if ($value.IsPresent) { $out += "-$key" }
    } elseif ($value -is [array]) {
      $out += "-$key"; $out += (ConvertTo-CommandArgument ($value -join ','))
    } else {
      $out += "-$key"; $out += (ConvertTo-CommandArgument ([string]$value))
    }
  }
  $out -join ' '
}

# Runs `$Script` with `$Bound` on a new desktop and returns its exit code, or $null when this
# process already runs on a test desktop or the person asked to watch.
function Invoke-OnOwnDesktop([string]$Script, $Bound) {
  if ($env:RIVET_TEST_DESKTOP -or $env:RIVET_TEST_SAME_DESKTOP -eq '1') { return $null }
  $name = "rivet-test-$PID"
  $desktop = [RivetTestDesktop]::Create($name)
  try {
    $env:RIVET_TEST_DESKTOP = "$([RivetTestDesktop]::Station())\$name"
    $pwsh = (Get-Process -Id $PID).Path
    $line = (ConvertTo-CommandArgument $pwsh) + ' -NoProfile -File ' + (ConvertTo-CommandArgument $Script) + ' ' + (ConvertTo-RunnerArguments $Bound)
    $runner = [RivetTestDesktop]::Start($env:RIVET_TEST_DESKTOP, $line, $true)
    [void]$runner.WaitForExit(-1)
    $code = $runner.ExitCode
    $runner.Dispose()
    return $code
  } finally {
    Remove-Item Env:\RIVET_TEST_DESKTOP -ErrorAction SilentlyContinue
    [void][RivetTestDesktop]::Close($desktop)
  }
}

# The browser frame window of the processes `$ProcessId`, or [IntPtr]::Zero while they have none.
function Get-BrowserWindow([int[]]$ProcessId) { [RivetTestDesktop]::BrowserWindow($ProcessId) }

# Starts `$File` with `$Arguments` on the person's own desktop, for the guard of a run that is on a
# test desktop. The result has Id, HasExited, ExitCode and WaitForExit(milliseconds).
function Start-OnDefaultDesktop([string]$File, [string[]]$Arguments) {
  $line = (ConvertTo-CommandArgument $File) + ' ' + (($Arguments | ForEach-Object { ConvertTo-CommandArgument $_ }) -join ' ')
  [RivetTestDesktop]::Start("$([RivetTestDesktop]::Station())\Default", $line, $false)
}
