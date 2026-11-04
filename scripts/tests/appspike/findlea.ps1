param([string]$Dll, [string]$Target)
# Finds `lea reg, [rip + disp]` instructions that point at an RVA, and the function each one sits in.
Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.IO;
public static class Lea{
 public static List<string> Find(string dll,uint target){
  var b=File.ReadAllBytes(dll);int pe=BitConverter.ToInt32(b,0x3c);int n=BitConverter.ToUInt16(b,pe+6);int opt=BitConverter.ToUInt16(b,pe+20);int sec=pe+24+opt;
  uint pd=BitConverter.ToUInt32(b,pe+24+112+24),pdSize=BitConverter.ToUInt32(b,pe+24+112+28);long pdOff=0;
  var secs=new List<uint[]>();
  for(int i=0;i<n;i++){int h=sec+40*i;secs.Add(new uint[]{BitConverter.ToUInt32(b,h+12),BitConverter.ToUInt32(b,h+8),BitConverter.ToUInt32(b,h+20),BitConverter.ToUInt32(b,h+16),BitConverter.ToUInt32(b,h+36)});}
  foreach(var s in secs){if(pd>=s[0]&&pd<s[0]+s[1])pdOff=s[2]+(pd-s[0]);}
  int cnt=(int)(pdSize/12);var starts=new uint[cnt];var ends=new uint[cnt];
  for(int k=0;k<cnt;k++){starts[k]=BitConverter.ToUInt32(b,(int)pdOff+12*k);ends[k]=BitConverter.ToUInt32(b,(int)pdOff+12*k+4);}
  var o=new List<string>();
  foreach(var s in secs){ if((s[4]&0x20000000)==0)continue; long end=s[2]+Math.Min(s[3],s[1])-7;
   for(long p=s[2];p<end;p++){
    byte r=b[p]; if(r!=0x48&&r!=0x4C)continue; if(b[p+1]!=0x8D)continue; if((b[p+2]&0xC7)!=0x05)continue;
    int rel=BitConverter.ToInt32(b,(int)p+3);long at=s[0]+(p-s[2]);long t=at+7+rel;
    if(t==target){ uint fs=0; int lo=0,hi=cnt-1; while(lo<=hi){int mid=(lo+hi)/2; if(starts[mid]<=at){fs=starts[mid];lo=mid+1;}else hi=mid-1;}
     o.Add(at.ToString("X8")+" in function "+fs.ToString("X8")+" (+"+(at-fs).ToString("X")+")"); }
   }}
  return o;}
}
'@
[Lea]::Find($Dll, [Convert]::ToUInt32($Target, 16))
