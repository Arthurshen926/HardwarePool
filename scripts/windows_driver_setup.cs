using System;
using System.ComponentModel;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

// Original user-mode SetupAPI interop. Loading this type performs no installation.
public static class CapyDriverSetup {
    [StructLayout(LayoutKind.Sequential)] struct DeviceInfo {
        public uint Size; public Guid Class; public uint DevInst; public UIntPtr Reserved;
        public static DeviceInfo New() { return new DeviceInfo { Size=(uint)Marshal.SizeOf(typeof(DeviceInfo)) }; }
    }
    [DllImport("setupapi.dll", SetLastError=true)] static extern IntPtr SetupDiCreateDeviceInfoList(ref Guid id, IntPtr parent);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr SetupDiGetClassDevsW(IntPtr id,string enumerator,IntPtr parent,uint flags);
    [DllImport("setupapi.dll", SetLastError=true)] static extern bool SetupDiEnumDeviceInfo(IntPtr set,uint index,ref DeviceInfo info);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool SetupDiGetDeviceRegistryPropertyW(IntPtr set,ref DeviceInfo info,uint property,out uint type,byte[] data,uint size,out uint needed);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool SetupDiGetINFClassW(string inf,out Guid id,StringBuilder name,uint size,out uint needed);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool SetupDiCreateDeviceInfoW(IntPtr set,string name,ref Guid id,string description,IntPtr parent,uint flags,ref DeviceInfo info);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool SetupDiSetDeviceRegistryPropertyW(IntPtr set,ref DeviceInfo info,uint property,byte[] data,uint size);
    [DllImport("setupapi.dll", SetLastError=true)] static extern bool SetupDiCallClassInstaller(uint operation,IntPtr set,ref DeviceInfo info);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool SetupDiGetDeviceInstanceIdW(IntPtr set,ref DeviceInfo info,StringBuilder id,uint size,out uint needed);
    [DllImport("setupapi.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool SetupDiOpenDeviceInfoW(IntPtr set,string instance,IntPtr parent,uint flags,ref DeviceInfo info);
    [DllImport("setupapi.dll", SetLastError=true)] static extern bool SetupDiDestroyDeviceInfoList(IntPtr set);
    [DllImport("newdev.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool DiInstallDriverW(IntPtr parent,string inf,uint flags,out bool reboot);
    [DllImport("newdev.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool UpdateDriverForPlugAndPlayDevicesW(IntPtr parent,string hardware,string inf,uint flags,out bool reboot);
    static void Check(bool ok) { if(!ok) throw new Win32Exception(Marshal.GetLastWin32Error()); }
    static string Instance(IntPtr set, ref DeviceInfo info) {
        var name=new StringBuilder(512); uint needed;
        Check(SetupDiGetDeviceInstanceIdW(set,ref info,name,512,out needed)); return name.ToString();
    }
    static void Allowed(string hardware) {
        if(hardware!="Root\\CapyIO_VirtualSpeaker" && hardware!="Root\\CapyIOVhfTouchpad") throw new ArgumentException("Not a CapyIO root hardware ID");
    }
    public static string[] Existing(string hardware) {
        Allowed(hardware); var found=new List<string>();
        IntPtr set=SetupDiGetClassDevsW(IntPtr.Zero,null,IntPtr.Zero,4);
        if(set==new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            for(uint i=0;;i++) {
                var info=DeviceInfo.New();
                if(!SetupDiEnumDeviceInfo(set,i,ref info)) { int error=Marshal.GetLastWin32Error(); if(error==259) break; throw new Win32Exception(error); }
                uint type,needed; byte[] data=new byte[8192];
                if(!SetupDiGetDeviceRegistryPropertyW(set,ref info,1,out type,data,(uint)data.Length,out needed)) {
                    int error=Marshal.GetLastWin32Error(); if(error==13) continue; throw new Win32Exception(error);
                }
                foreach(string id in Encoding.Unicode.GetString(data,0,(int)needed).Split('\0')) {
                    if(String.Equals(id,hardware,StringComparison.OrdinalIgnoreCase)) { found.Add(Instance(set,ref info)); break; }
                }
            }
        } finally { SetupDiDestroyDeviceInfoList(set); }
        return found.ToArray();
    }
    public static string CreateRoot(string inf,string hardware) {
        Allowed(hardware);
        if(Existing(hardware).Length!=0) throw new InvalidOperationException("Root device already exists; refusing a duplicate");
        Guid id; uint needed; var name=new StringBuilder(256);
        Check(SetupDiGetINFClassW(inf,out id,name,256,out needed));
        Guid expected=new Guid(hardware.EndsWith("VirtualSpeaker")?"4d36e96c-e325-11ce-bfc1-08002be10318":"4d36e97d-e325-11ce-bfc1-08002be10318");
        if(id!=expected) throw new InvalidOperationException("INF class differs from the fixed component contract");
        IntPtr set=SetupDiCreateDeviceInfoList(ref id,IntPtr.Zero);
        if(set==new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
        var info=DeviceInfo.New(); bool registered=false;
        try {
            Check(SetupDiCreateDeviceInfoW(set,name.ToString(),ref id,"CapyIO",IntPtr.Zero,1,ref info));
            byte[] ids=Encoding.Unicode.GetBytes(hardware+"\0\0");
            Check(SetupDiSetDeviceRegistryPropertyW(set,ref info,1,ids,(uint)ids.Length));
            Check(SetupDiCallClassInstaller(0x19,set,ref info)); registered=true;
            return Instance(set,ref info);
        } catch {
            if(registered) Check(SetupDiCallClassInstaller(5,set,ref info));
            throw;
        } finally { SetupDiDestroyDeviceInfoList(set); }
    }
    public static bool Stage(string inf) { bool reboot; Check(DiInstallDriverW(IntPtr.Zero,inf,0,out reboot)); return reboot; }
    public static bool Bind(string inf,string hardware) { Allowed(hardware); bool reboot;
        if(!UpdateDriverForPlugAndPlayDevicesW(IntPtr.Zero,hardware,inf,0,out reboot)) {
            int error=Marshal.GetLastWin32Error();
            // PnP may already have selected the staged package after root creation.
            // Do not force a downgrade; the caller still requires device health.
            if(error!=259) throw new Win32Exception(error);
        }
        return reboot; }
    public static void RemoveCreated(string instance) {
        if(!instance.StartsWith("ROOT\\",StringComparison.OrdinalIgnoreCase)) throw new ArgumentException("Expected an exact created root instance");
        IntPtr set=SetupDiGetClassDevsW(IntPtr.Zero,null,IntPtr.Zero,4);
        if(set==new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
        try { var info=DeviceInfo.New(); Check(SetupDiOpenDeviceInfoW(set,instance,IntPtr.Zero,0,ref info)); Check(SetupDiCallClassInstaller(5,set,ref info)); }
        finally { SetupDiDestroyDeviceInfoList(set); }
    }
}
