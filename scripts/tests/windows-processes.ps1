# The process list for the browser tests, read without WMI. Polling Win32_Process through CIM keeps
# WmiPrvSE busy and slows the whole machine, so the list comes from a Toolhelp snapshot instead.
# A process's command line and image path are read once, when its id and start time are first seen,
# and kept until it exits. Start time is part of the key because Windows reuses process ids.
#
#   . scripts/tests/windows-processes.ps1
#   Get-ProcessList [-Name whale.exe]
#
# Each item has the Win32_Process names the scripts used before: ProcessId, ParentProcessId, Name,
# CommandLine, ExecutablePath and CreationDate. CommandLine, ExecutablePath and CreationDate are
# null for a process this user may not open, as they are through CIM.
if (-not ('RivetProcesses' -as [type])) {
  Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.Runtime.InteropServices;
public class RivetProcess{
 public uint ProcessId;public uint ParentProcessId;public string Name;public string CommandLine;public string ExecutablePath;public object CreationDate;
}
public static class RivetProcesses{
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct PE{public uint dwSize,cntUsage,th32ProcessID;public IntPtr th32DefaultHeapID;public uint th32ModuleID,cntThreads,th32ParentProcessID;public int pcPriClassBase;public uint dwFlags;[MarshalAs(UnmanagedType.ByValTStr,SizeConst=260)]public string szExeFile;}
 [StructLayout(LayoutKind.Sequential)] struct US{public ushort Length,MaximumLength;public IntPtr Buffer;}
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr CreateToolhelp32Snapshot(uint f,uint p);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool Process32FirstW(IntPtr s,ref PE e);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool Process32NextW(IntPtr s,ref PE e);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint a,bool i,uint p);
 [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr h,out long c,out long e,out long k,out long u);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool QueryFullProcessImageNameW(IntPtr h,uint f,System.Text.StringBuilder s,ref uint n);
 [DllImport("ntdll.dll")] static extern int NtQueryInformationProcess(IntPtr h,int c,IntPtr b,int n,out int r);
 const uint QueryLimited=0x1000;
 static readonly Dictionary<string,RivetProcess> cache=new Dictionary<string,RivetProcess>();
 static string ReadCommandLine(IntPtr h){
  // ProcessCommandLineInformation: a UNICODE_STRING followed by its text. Needs only limited query access.
  int n;NtQueryInformationProcess(h,60,IntPtr.Zero,0,out n);
  if(n<=0)return null;
  IntPtr b=Marshal.AllocHGlobal(n);
  try{
   if(NtQueryInformationProcess(h,60,b,n,out n)!=0)return null;
   var u=(US)Marshal.PtrToStructure(b,typeof(US));
   return u.Buffer==IntPtr.Zero?"":Marshal.PtrToStringUni(u.Buffer,u.Length/2);
  }finally{Marshal.FreeHGlobal(b);}
 }
 public static List<RivetProcess> Snapshot(){
  var list=new List<RivetProcess>();var alive=new HashSet<string>();
  IntPtr s=CreateToolhelp32Snapshot(2,0);
  if(s==IntPtr.Zero||s==new IntPtr(-1))return list;
  try{
   var e=new PE();e.dwSize=(uint)Marshal.SizeOf(typeof(PE));
   for(bool ok=Process32FirstW(s,ref e);ok;ok=Process32NextW(s,ref e)){
    long created=0;IntPtr h=OpenProcess(QueryLimited,false,e.th32ProcessID);
    try{
     long x,k,u;if(h!=IntPtr.Zero&&!GetProcessTimes(h,out created,out x,out k,out u))created=0;
     string key=e.th32ProcessID+"|"+created+"|"+e.szExeFile;alive.Add(key);
     RivetProcess p;
     if(!cache.TryGetValue(key,out p)){
      p=new RivetProcess{ProcessId=e.th32ProcessID,Name=e.szExeFile};
      if(h!=IntPtr.Zero){
       p.CommandLine=ReadCommandLine(h);
       var sb=new System.Text.StringBuilder(1024);uint n=1024;
       if(QueryFullProcessImageNameW(h,0,sb,ref n))p.ExecutablePath=sb.ToString();
       if(created!=0)p.CreationDate=DateTime.FromFileTime(created);
      }
      cache[key]=p;
     }
     p.ParentProcessId=e.th32ParentProcessID;
     list.Add(p);
    }finally{if(h!=IntPtr.Zero)CloseHandle(h);}
   }
  }finally{CloseHandle(s);}
  var gone=new List<string>();foreach(var k in cache.Keys)if(!alive.Contains(k))gone.Add(k);
  foreach(var k in gone)cache.Remove(k);
  return list;
 }
}
'@
}

function Get-ProcessList([string]$Name) {
  $all = [RivetProcesses]::Snapshot()
  if ($Name) { @($all | Where-Object { $_.Name -eq $Name }) } else { @($all) }
}

# Gigabytes free on the drive that holds `Path`.
function Get-FreeGigabytes([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path)
  [math]::Round((New-Object IO.DriveInfo ([IO.Path]::GetPathRoot($full))).AvailableFreeSpace / 1GB, 1)
}
