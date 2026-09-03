using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

internal static class HidReportProbe
{
    private const uint DigcfPresent = 0x00000002;
    private const uint DigcfDeviceInterface = 0x00000010;
    private const uint GenericRead = 0x80000000;
    private const uint GenericWrite = 0x40000000;
    private const uint FileShareRead = 0x00000001;
    private const uint FileShareWrite = 0x00000002;
    private const uint OpenExisting = 3;
    private const uint FileFlagOverlapped = 0x40000000;
    private const int ErrorIoPending = 997;
    private const uint WaitObject0 = 0;
    private const uint RimTypeHid = 2;
    private const uint RidiDeviceInfo = 0x2000000b;
    private const int MinimumHoldSeconds = 5;
    private const int MaximumHoldSeconds = 300;

    private static int Main(string[] arguments)
    {
        try
        {
            if (arguments.Length == 1 && arguments[0] == "--self-test")
            {
                return SelfTest();
            }
            if (arguments.Length != 3)
            {
                throw new ArgumentException(
                    "usage: CapyIO.HidReportProbe.exe <vendor-id-hex> <product-id-hex> <hold-seconds>");
            }
            ushort vendorId = ParseHex(arguments[0], "vendor ID");
            ushort productId = ParseHex(arguments[1], "product ID");
            int holdSeconds;
            if (!Int32.TryParse(arguments[2], NumberStyles.None, CultureInfo.InvariantCulture, out holdSeconds)
                || holdSeconds < MinimumHoldSeconds
                || holdSeconds > MaximumHoldSeconds)
            {
                throw new ArgumentException(
                    "hold seconds must be within " + MinimumHoldSeconds + "..=" + MaximumHoldSeconds);
            }
            return Run(vendorId, productId, holdSeconds);
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("CAPYIO_HID_REPORT_PROBE_FAILED: " + error.Message);
            return 2;
        }
    }

    private static int Run(ushort vendorId, ushort productId, int holdSeconds)
    {
        ProbeRawInputInventory(vendorId, productId);
        List<HidInterface> matches = FindInterfaces(vendorId, productId);
        if (matches.Count != 1)
        {
            throw new InvalidOperationException(
                "expected exactly one " + vendorId.ToString("x4") + ":" + productId.ToString("x4")
                + " HID input interface, found " + matches.Count);
        }
        HidInterface target = matches[0];
        Console.WriteLine(
            "CAPYIO_HID_DEVICE={0:x4}:{1:x4}:version={2:x4}:usage_page={3:x4}:usage={4:x4}:input_report_bytes={5}",
            vendorId,
            productId,
            target.VersionNumber,
            target.UsagePage,
            target.Usage,
            target.InputReportBytes);
        ProbeChromiumCompatibleOpen(target.Path);

        using (SafeFileHandle handle = CreateFile(
            target.Path,
            GenericRead,
            FileShareRead | FileShareWrite,
            IntPtr.Zero,
            OpenExisting,
            FileFlagOverlapped,
            IntPtr.Zero))
        {
            if (handle.IsInvalid)
            {
                throw new InvalidOperationException(
                    "could not open the exact HID input interface: Win32 " + Marshal.GetLastWin32Error());
            }
            DateTime deadline = DateTime.UtcNow.AddSeconds(holdSeconds);
            byte[] baseline = null;
            ulong reports = 0;
            ulong changedReports = 0;
            ulong readTimeouts = 0;
            while (DateTime.UtcNow < deadline)
            {
                byte[] report;
                if (!TryReadReport(handle, target.InputReportBytes, 1000, out report))
                {
                    readTimeouts++;
                    continue;
                }
                reports++;
                if (baseline == null)
                {
                    baseline = report;
                    Console.WriteLine(
                        "CAPYIO_HID_FIRST_REPORT_PREFIX="
                        + BitConverter.ToString(report, 0, Math.Min(8, report.Length)));
                }
                else if (Different(report, baseline))
                {
                    changedReports++;
                }
            }
            Console.WriteLine(
                "CAPYIO_HID_REPORT_RESULT=reports={0} changed_reports={1} read_timeouts={2}",
                reports,
                changedReports,
                readTimeouts);
            if (reports == 0)
            {
                throw new InvalidOperationException("the HID interface produced no input report");
            }
            if (changedReports == 0)
            {
                throw new InvalidOperationException("the HID input report did not change during the probe");
            }
        }
        Console.WriteLine("CAPYIO_HID_REPORTS_PASSED");
        return 0;
    }

    private static void ProbeRawInputInventory(ushort vendorId, ushort productId)
    {
        uint count = 0;
        uint listSize = (uint)Marshal.SizeOf(typeof(RawInputDeviceList));
        if (GetRawInputDeviceList(null, ref count, listSize) != 0)
        {
            throw new InvalidOperationException(
                "could not size RawInput inventory: Win32 " + Marshal.GetLastWin32Error());
        }
        RawInputDeviceList[] devices = new RawInputDeviceList[count];
        uint returned = GetRawInputDeviceList(devices, ref count, listSize);
        if (returned == UInt32.MaxValue)
        {
            throw new InvalidOperationException(
                "could not enumerate RawInput inventory: Win32 " + Marshal.GetLastWin32Error());
        }
        uint matches = 0;
        for (int index = 0; index < returned; index++)
        {
            if (devices[index].Type != RimTypeHid)
            {
                continue;
            }
            RawInputDeviceInfo info = new RawInputDeviceInfo();
            info.Size = (uint)Marshal.SizeOf(typeof(RawInputDeviceInfo));
            uint infoSize = info.Size;
            if (GetRawInputDeviceInfo(
                devices[index].Device,
                RidiDeviceInfo,
                ref info,
                ref infoSize) == UInt32.MaxValue)
            {
                continue;
            }
            if (info.Hid.UsagePage == 0x0001
                && (info.Hid.Usage == 0x0004
                    || info.Hid.Usage == 0x0005
                    || info.Hid.Usage == 0x0008))
            {
                Console.WriteLine(
                    "CAPYIO_RAW_INPUT_GAMEPAD_CANDIDATE={0:x4}:{1:x4}:version={2:x4}:usage={3:x4}",
                    info.Hid.VendorId,
                    info.Hid.ProductId,
                    info.Hid.VersionNumber,
                    info.Hid.Usage);
            }
            if (info.Hid.VendorId == vendorId && info.Hid.ProductId == productId)
            {
                matches++;
                Console.WriteLine(
                    "CAPYIO_RAW_INPUT_DEVICE={0:x4}:{1:x4}:version={2:x4}:usage_page={3:x4}:usage={4:x4}",
                    info.Hid.VendorId,
                    info.Hid.ProductId,
                    info.Hid.VersionNumber,
                    info.Hid.UsagePage,
                    info.Hid.Usage);
            }
        }
        Console.WriteLine(
            "CAPYIO_RAW_INPUT_INVENTORY=devices={0} target_matches={1}",
            returned,
            matches);
    }

    private static void ProbeChromiumCompatibleOpen(string path)
    {
        using (SafeFileHandle handle = CreateFile(
            path,
            GenericRead | GenericWrite,
            FileShareRead | FileShareWrite,
            IntPtr.Zero,
            OpenExisting,
            0,
            IntPtr.Zero))
        {
            if (handle.IsInvalid)
            {
                Console.WriteLine(
                    "CAPYIO_HID_READ_WRITE_OPEN=failed_win32_" + Marshal.GetLastWin32Error());
                return;
            }
            StringBuilder product = new StringBuilder(126);
            bool productAvailable = HidD_GetProductString(handle, product, product.Capacity * 2);
            Console.WriteLine(
                "CAPYIO_HID_READ_WRITE_OPEN=passed product_string={0}",
                productAvailable && product.Length > 0 ? product.ToString() : "unavailable");
        }
    }

    private static List<HidInterface> FindInterfaces(ushort vendorId, ushort productId)
    {
        Guid hidGuid;
        HidD_GetHidGuid(out hidGuid);
        IntPtr deviceSet = SetupDiGetClassDevs(
            ref hidGuid,
            null,
            IntPtr.Zero,
            DigcfPresent | DigcfDeviceInterface);
        if (deviceSet == new IntPtr(-1))
        {
            throw new InvalidOperationException(
                "could not enumerate HID interfaces: Win32 " + Marshal.GetLastWin32Error());
        }
        try
        {
            List<HidInterface> matches = new List<HidInterface>();
            uint index = 0;
            while (true)
            {
                SpDeviceInterfaceData interfaceData = new SpDeviceInterfaceData();
                interfaceData.Size = Marshal.SizeOf(typeof(SpDeviceInterfaceData));
                if (!SetupDiEnumDeviceInterfaces(
                    deviceSet,
                    IntPtr.Zero,
                    ref hidGuid,
                    index,
                    ref interfaceData))
                {
                    int error = Marshal.GetLastWin32Error();
                    if (error == 259)
                    {
                        break;
                    }
                    throw new InvalidOperationException("HID interface enumeration failed: Win32 " + error);
                }
                index++;
                string path = GetInterfacePath(deviceSet, ref interfaceData);
                string identity = "vid_" + vendorId.ToString("x4")
                    + "&pid_" + productId.ToString("x4");
                bool exactPath = path.IndexOf(identity, StringComparison.OrdinalIgnoreCase) >= 0;
                if (exactPath)
                {
                    Console.WriteLine("CAPYIO_HID_CANDIDATE_PATH=" + path);
                }
                using (SafeFileHandle handle = CreateFile(
                    path,
                    0,
                    FileShareRead | FileShareWrite,
                    IntPtr.Zero,
                    OpenExisting,
                    0,
                    IntPtr.Zero))
                {
                    if (handle.IsInvalid)
                    {
                        if (exactPath)
                        {
                            Console.WriteLine(
                                "CAPYIO_HID_CANDIDATE_OPEN_FAILED_WIN32="
                                + Marshal.GetLastWin32Error());
                        }
                        continue;
                    }
                    HiddAttributes attributes = new HiddAttributes();
                    attributes.Size = Marshal.SizeOf(typeof(HiddAttributes));
                    if (!HidD_GetAttributes(handle, ref attributes))
                    {
                        if (exactPath)
                        {
                            Console.WriteLine(
                                "CAPYIO_HID_CANDIDATE_ATTRIBUTES_FAILED_WIN32="
                                + Marshal.GetLastWin32Error());
                        }
                        continue;
                    }
                    if (exactPath)
                    {
                        Console.WriteLine(
                            "CAPYIO_HID_CANDIDATE_ATTRIBUTES={0:x4}:{1:x4}",
                            attributes.VendorId,
                            attributes.ProductId);
                    }
                    if (attributes.VendorId != vendorId
                        || attributes.ProductId != productId)
                    {
                        continue;
                    }
                    IntPtr preparsed;
                    if (!HidD_GetPreparsedData(handle, out preparsed))
                    {
                        throw new InvalidOperationException("could not read HID preparsed data");
                    }
                    try
                    {
                        HidpCaps caps;
                        int status = HidP_GetCaps(preparsed, out caps);
                        if (status < 0 || caps.InputReportByteLength == 0)
                        {
                            throw new InvalidOperationException(
                                "could not read HID input capabilities: NTSTATUS 0x"
                                + status.ToString("x8"));
                        }
                        matches.Add(new HidInterface(
                            path,
                            attributes.VersionNumber,
                            caps.UsagePage,
                            caps.Usage,
                            caps.InputReportByteLength));
                    }
                    finally
                    {
                        HidD_FreePreparsedData(preparsed);
                    }
                }
            }
            return matches;
        }
        finally
        {
            SetupDiDestroyDeviceInfoList(deviceSet);
        }
    }

    private static string GetInterfacePath(
        IntPtr deviceSet,
        ref SpDeviceInterfaceData interfaceData)
    {
        uint requiredSize = 0;
        SetupDiGetDeviceInterfaceDetail(
            deviceSet,
            ref interfaceData,
            IntPtr.Zero,
            0,
            ref requiredSize,
            IntPtr.Zero);
        if (requiredSize == 0)
        {
            throw new InvalidOperationException(
                "could not size HID interface path: Win32 " + Marshal.GetLastWin32Error());
        }
        IntPtr detail = Marshal.AllocHGlobal(checked((int)requiredSize));
        try
        {
            Marshal.WriteInt32(detail, IntPtr.Size == 8 ? 8 : 6);
            if (!SetupDiGetDeviceInterfaceDetail(
                deviceSet,
                ref interfaceData,
                detail,
                requiredSize,
                ref requiredSize,
                IntPtr.Zero))
            {
                throw new InvalidOperationException(
                    "could not read HID interface path: Win32 " + Marshal.GetLastWin32Error());
            }
            // SP_DEVICE_INTERFACE_DETAIL_DATA_W has an x64 cbSize of 8 for ABI
            // alignment, but its variable UTF-16 DevicePath still begins
            // immediately after the four-byte DWORD.
            IntPtr path = IntPtr.Add(detail, 4);
            return Marshal.PtrToStringUni(path);
        }
        finally
        {
            Marshal.FreeHGlobal(detail);
        }
    }

    private static bool TryReadReport(
        SafeFileHandle handle,
        int reportLength,
        int timeoutMilliseconds,
        out byte[] report)
    {
        report = null;
        IntPtr buffer = Marshal.AllocHGlobal(reportLength);
        IntPtr completed = CreateEvent(IntPtr.Zero, true, false, null);
        if (completed == IntPtr.Zero)
        {
            Marshal.FreeHGlobal(buffer);
            throw new InvalidOperationException("could not create HID read event");
        }
        IntPtr overlapped = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(NativeOverlappedData)));
        try
        {
            NativeOverlappedData data = new NativeOverlappedData();
            data.Event = completed;
            Marshal.StructureToPtr(data, overlapped, false);
            bool started = ReadFile(handle, buffer, reportLength, IntPtr.Zero, overlapped);
            if (!started && Marshal.GetLastWin32Error() != ErrorIoPending)
            {
                throw new InvalidOperationException(
                    "HID ReadFile failed: Win32 " + Marshal.GetLastWin32Error());
            }
            uint wait = WaitForSingleObject(completed, checked((uint)timeoutMilliseconds));
            if (wait != WaitObject0)
            {
                if (!CancelIoEx(handle, overlapped) && Marshal.GetLastWin32Error() != 1168)
                {
                    throw new InvalidOperationException(
                        "could not cancel timed-out HID read: Win32 " + Marshal.GetLastWin32Error());
                }
                uint cancelledBytes;
                GetOverlappedResult(handle, overlapped, out cancelledBytes, true);
                return false;
            }
            uint bytesRead;
            if (!GetOverlappedResult(handle, overlapped, out bytesRead, false))
            {
                throw new InvalidOperationException(
                    "HID overlapped result failed: Win32 " + Marshal.GetLastWin32Error());
            }
            if (bytesRead == 0 || bytesRead > reportLength)
            {
                return false;
            }
            report = new byte[bytesRead];
            Marshal.Copy(buffer, report, 0, checked((int)bytesRead));
            return true;
        }
        finally
        {
            Marshal.FreeHGlobal(overlapped);
            CloseHandle(completed);
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static int SelfTest()
    {
        if (ParseHex("09cc", "product ID") != 0x09cc
            || !Different(new byte[] { 1, 2 }, new byte[] { 1, 3 })
            || Different(new byte[] { 1, 2 }, new byte[] { 1, 2 }))
        {
            throw new InvalidOperationException("HID report probe self-test failed");
        }
        Console.WriteLine("CAPYIO_HID_REPORT_SELF_TEST_PASSED");
        return 0;
    }

    private static ushort ParseHex(string value, string label)
    {
        ushort parsed;
        if (!UInt16.TryParse(value, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out parsed))
        {
            throw new ArgumentException("invalid hexadecimal " + label);
        }
        return parsed;
    }

    private static bool Different(byte[] current, byte[] baseline)
    {
        if (current.Length != baseline.Length)
        {
            return true;
        }
        for (int index = 0; index < current.Length; index++)
        {
            if (current[index] != baseline[index])
            {
                return true;
            }
        }
        return false;
    }

    private sealed class HidInterface
    {
        internal HidInterface(
            string path,
            ushort versionNumber,
            ushort usagePage,
            ushort usage,
            ushort inputReportBytes)
        {
            Path = path;
            VersionNumber = versionNumber;
            UsagePage = usagePage;
            Usage = usage;
            InputReportBytes = inputReportBytes;
        }

        internal string Path { get; private set; }
        internal ushort VersionNumber { get; private set; }
        internal ushort UsagePage { get; private set; }
        internal ushort Usage { get; private set; }
        internal ushort InputReportBytes { get; private set; }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SpDeviceInterfaceData
    {
        internal int Size;
        internal Guid InterfaceClassGuid;
        internal uint Flags;
        internal IntPtr Reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct HiddAttributes
    {
        internal int Size;
        internal ushort VendorId;
        internal ushort ProductId;
        internal ushort VersionNumber;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RawInputDeviceList
    {
        internal IntPtr Device;
        internal uint Type;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RawInputHidInfo
    {
        internal uint VendorId;
        internal uint ProductId;
        internal uint VersionNumber;
        internal ushort UsagePage;
        internal ushort Usage;
    }

    // RID_DEVICE_INFO contains a union whose keyboard member is 24 bytes;
    // retaining only the 16-byte HID view without the native 32-byte outer
    // size makes RIDI_DEVICEINFO reject every device on Windows.
    [StructLayout(LayoutKind.Explicit, Size = 32)]
    private struct RawInputDeviceInfo
    {
        [FieldOffset(0)]
        internal uint Size;
        [FieldOffset(4)]
        internal uint Type;
        [FieldOffset(8)]
        internal RawInputHidInfo Hid;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct HidpCaps
    {
        internal ushort Usage;
        internal ushort UsagePage;
        internal ushort InputReportByteLength;
        internal ushort OutputReportByteLength;
        internal ushort FeatureReportByteLength;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 17)]
        internal ushort[] Reserved;
        internal ushort NumberLinkCollectionNodes;
        internal ushort NumberInputButtonCaps;
        internal ushort NumberInputValueCaps;
        internal ushort NumberInputDataIndices;
        internal ushort NumberOutputButtonCaps;
        internal ushort NumberOutputValueCaps;
        internal ushort NumberOutputDataIndices;
        internal ushort NumberFeatureButtonCaps;
        internal ushort NumberFeatureValueCaps;
        internal ushort NumberFeatureDataIndices;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeOverlappedData
    {
        internal IntPtr Internal;
        internal IntPtr InternalHigh;
        internal uint Offset;
        internal uint OffsetHigh;
        internal IntPtr Event;
    }

    [DllImport("hid.dll")]
    private static extern void HidD_GetHidGuid(out Guid hidGuid);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_GetAttributes(
        SafeFileHandle handle,
        ref HiddAttributes attributes);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_GetPreparsedData(
        SafeFileHandle handle,
        out IntPtr preparsedData);

    [DllImport("hid.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool HidD_GetProductString(
        SafeFileHandle handle,
        StringBuilder productString,
        int productStringBytes);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint GetRawInputDeviceList(
        [Out] RawInputDeviceList[] devices,
        ref uint deviceCount,
        uint structureSize);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint GetRawInputDeviceInfo(
        IntPtr device,
        uint command,
        ref RawInputDeviceInfo data,
        ref uint dataSize);

    [DllImport("hid.dll")]
    private static extern bool HidD_FreePreparsedData(IntPtr preparsedData);

    [DllImport("hid.dll")]
    private static extern int HidP_GetCaps(IntPtr preparsedData, out HidpCaps capabilities);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr SetupDiGetClassDevs(
        ref Guid classGuid,
        string enumerator,
        IntPtr parent,
        uint flags);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiEnumDeviceInterfaces(
        IntPtr deviceInfoSet,
        IntPtr deviceInfoData,
        ref Guid interfaceClassGuid,
        uint memberIndex,
        ref SpDeviceInterfaceData deviceInterfaceData);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetupDiGetDeviceInterfaceDetail(
        IntPtr deviceInfoSet,
        ref SpDeviceInterfaceData deviceInterfaceData,
        IntPtr deviceInterfaceDetailData,
        uint deviceInterfaceDetailDataSize,
        ref uint requiredSize,
        IntPtr deviceInfoData);

    [DllImport("setupapi.dll")]
    private static extern bool SetupDiDestroyDeviceInfoList(IntPtr deviceInfoSet);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFile(
        string fileName,
        uint desiredAccess,
        uint shareMode,
        IntPtr securityAttributes,
        uint creationDisposition,
        uint flagsAndAttributes,
        IntPtr templateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool ReadFile(
        SafeFileHandle file,
        IntPtr buffer,
        int bytesToRead,
        IntPtr bytesRead,
        IntPtr overlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetOverlappedResult(
        SafeFileHandle file,
        IntPtr overlapped,
        out uint bytesTransferred,
        bool wait);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CancelIoEx(SafeFileHandle file, IntPtr overlapped);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateEvent(
        IntPtr eventAttributes,
        bool manualReset,
        bool initialState,
        string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll")]
    private static extern bool CloseHandle(IntPtr handle);
}
