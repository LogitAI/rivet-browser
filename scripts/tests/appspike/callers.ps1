param([string]$Dll, [string]$Rva, [int]$Before = 24)
Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.IO;
public static class Callers{
 public static List<string> Find(string dll,uint target,int before){
  var b=File.ReadAllBytes(dll);int pe=BitConverter.ToInt32(b,0x3c);int n=BitConverter.ToUInt16(b,pe+6);int opt=BitConverter.ToUInt16(b,pe+20);int sec=pe+24+opt;
  var o=new List<string>();
  for(int i=0;i<n;i++){int h=sec+40*i;uint vs=BitConverter.ToUInt32(b,h+8),va=BitConverter.ToUInt32(b,h+12),rs=BitConverter.ToUInt32(b,h+16),ro=BitConverter.ToUInt32(b,h+20),ch=BitConverter.ToUInt32(b,h+36);
   if((ch&0x20000000)==0)continue;long end=ro+Math.Min(rs,vs)-5;
   for(long p=ro;p<end;p++){if(b[p]!=0xE8)continue;int rel=BitConverter.ToInt32(b,(int)p+1);long at=va+(p-ro);long t=at+5+rel;if(t==target){var s=new System.Text.StringBuilder();s.Append(at.ToString("X8")+": ");for(long k=p-before;k<p+5;k++)s.Append(b[k].ToString("X2")+" ");o.Add(s.ToString());}}
  }
  return o;}
}
'@
[Callers]::Find($Dll, [Convert]::ToUInt32($Rva, 16), $Before)
