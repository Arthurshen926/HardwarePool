using System;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Threading;

internal static class XInputProbe
{
    private const int ErrorSuccess = 0;

    private static int Main(string[] arguments)
    {
        try
        {
            if (arguments.Length == 1 && arguments[0] == "--self-test")
            {
                Console.WriteLine("CAPYIO_XINPUT_SELF_TEST_PASSED");
                return 0;
            }
            int seconds;
            if (arguments.Length != 1
                || !Int32.TryParse(arguments[0], NumberStyles.None, CultureInfo.InvariantCulture, out seconds)
                || seconds < 5
                || seconds > 300)
            {
                throw new ArgumentException("usage: CapyIO.XInputProbe.exe <hold-seconds:5..300>");
            }
            return Run(seconds);
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("CAPYIO_XINPUT_PROBE_FAILED: " + error.Message);
            return 2;
        }
    }

    private static int Run(int seconds)
    {
        uint? selected = null;
        XInputState baseline = new XInputState();
        for (uint index = 0; index < 4; index++)
        {
            XInputState state;
            if (XInputGetState(index, out state) == ErrorSuccess)
            {
                if (selected.HasValue)
                {
                    throw new InvalidOperationException("expected exactly one connected XInput device");
                }
                selected = index;
                baseline = state;
            }
        }
        if (!selected.HasValue)
        {
            throw new InvalidOperationException("no connected XInput device was found");
        }
        Console.WriteLine("CAPYIO_XINPUT_DEVICE=user_index=" + selected.Value);
        Console.WriteLine("CAPYIO_XINPUT_BASELINE_PACKET=" + baseline.PacketNumber);
        DateTime deadline = DateTime.UtcNow.AddSeconds(seconds);
        uint samples = 0;
        uint packetAdvances = 0;
        bool changed = false;
        XInputState previous = baseline;
        while (DateTime.UtcNow < deadline)
        {
            XInputState current;
            if (XInputGetState(selected.Value, out current) != ErrorSuccess)
            {
                throw new InvalidOperationException("the selected XInput device disconnected");
            }
            samples++;
            if (current.PacketNumber != previous.PacketNumber)
            {
                packetAdvances++;
            }
            changed |= !current.Gamepad.Equals(baseline.Gamepad);
            previous = current;
            Thread.Sleep(4);
        }
        Console.WriteLine(
            "CAPYIO_XINPUT_RESULT=samples=" + samples
            + " packet_advances=" + packetAdvances
            + " changed=" + changed.ToString().ToLowerInvariant());
        if (!changed || packetAdvances == 0)
        {
            throw new InvalidOperationException("XInput state did not change during the probe");
        }
        Console.WriteLine("CAPYIO_XINPUT_REPORTS_PASSED");
        return 0;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct XInputState
    {
        internal uint PacketNumber;
        internal XInputGamepad Gamepad;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct XInputGamepad : IEquatable<XInputGamepad>
    {
        internal ushort Buttons;
        internal byte LeftTrigger;
        internal byte RightTrigger;
        internal short LeftThumbX;
        internal short LeftThumbY;
        internal short RightThumbX;
        internal short RightThumbY;

        public bool Equals(XInputGamepad other)
        {
            return Buttons == other.Buttons
                && LeftTrigger == other.LeftTrigger
                && RightTrigger == other.RightTrigger
                && LeftThumbX == other.LeftThumbX
                && LeftThumbY == other.LeftThumbY
                && RightThumbX == other.RightThumbX
                && RightThumbY == other.RightThumbY;
        }
    }

    [DllImport("xinput1_4.dll")]
    private static extern int XInputGetState(uint userIndex, out XInputState state);
}
