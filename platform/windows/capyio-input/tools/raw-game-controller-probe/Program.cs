using System;
using System.Globalization;
using System.Threading;
using Windows.Gaming.Input;

internal static class Program
{
    private const int MinimumHoldSeconds = 5;
    private const int MaximumHoldSeconds = 300;
    private const int DiscoveryMilliseconds = 5000;
    private const int DiscoveryPollMilliseconds = 50;

    private static int Main(string[] arguments)
    {
        try
        {
            return Run(arguments);
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("CAPYIO_RAW_GAMEPAD_PROBE_FAILED: " + error.Message);
            return 2;
        }
    }

    private static int Run(string[] arguments)
    {
        if (arguments.Length == 1 && arguments[0] == "--self-test")
        {
            return SelfTest();
        }
        if (arguments.Length != 3)
        {
            throw new ArgumentException(
                "usage: CapyIO.RawGameControllerProbe.exe <vendor-id-hex> <product-id-hex> <hold-seconds>");
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

        int matches;
        int inventoryCount;
        int discoverySnapshots;
        RawGameController target = DiscoverController(
            vendorId,
            productId,
            out matches,
            out inventoryCount,
            out discoverySnapshots);
        Console.WriteLine(
            "CAPYIO_RAW_GAMEPAD_DISCOVERY=milliseconds={0} snapshots={1} inventory={2} target_matches={3}",
            DiscoveryMilliseconds,
            discoverySnapshots,
            inventoryCount,
            matches);
        if (matches != 1 || target == null)
        {
            throw new InvalidOperationException(
                "expected exactly one " + vendorId.ToString("x4") + ":" + productId.ToString("x4")
                + " RawGameController, found " + matches);
        }

        bool[] buttons = new bool[checked((int)target.ButtonCount)];
        GameControllerSwitchPosition[] switches =
            new GameControllerSwitchPosition[checked((int)target.SwitchCount)];
        double[] axes = new double[checked((int)target.AxisCount)];
        ulong baselineTimestamp = target.GetCurrentReading(buttons, switches, axes);
        bool[] baselineButtons = (bool[])buttons.Clone();
        GameControllerSwitchPosition[] baselineSwitches =
            (GameControllerSwitchPosition[])switches.Clone();
        double[] baselineAxes = (double[])axes.Clone();

        Console.WriteLine(
            "CAPYIO_RAW_GAMEPAD_DEVICE={0:x4}:{1:x4}:{2}",
            target.HardwareVendorId,
            target.HardwareProductId,
            target.DisplayName);
        Console.WriteLine(
            "CAPYIO_RAW_GAMEPAD_LAYOUT=buttons={0} switches={1} axes={2}",
            buttons.Length,
            switches.Length,
            axes.Length);
        Gamepad standardGamepad = Gamepad.FromGameController(target);
        Console.WriteLine(
            "CAPYIO_RAW_GAMEPAD_STANDARD_MAPPING=" + Lower(standardGamepad != null));
        Console.WriteLine("CAPYIO_RAW_GAMEPAD_BASELINE_TIMESTAMP=" + baselineTimestamp);

        DateTime deadline = DateTime.UtcNow.AddSeconds(holdSeconds);
        ulong latestTimestamp = baselineTimestamp;
        ulong timestampAdvances = 0;
        ulong samples = 0;
        bool buttonChanged = false;
        bool switchChanged = false;
        bool axisChanged = false;
        bool finiteAxes = true;
        while (DateTime.UtcNow < deadline)
        {
            ulong timestamp = target.GetCurrentReading(buttons, switches, axes);
            samples++;
            if (timestamp > latestTimestamp)
            {
                latestTimestamp = timestamp;
                timestampAdvances++;
            }
            buttonChanged = buttonChanged || Different(buttons, baselineButtons);
            switchChanged = switchChanged || Different(switches, baselineSwitches);
            axisChanged = axisChanged || Different(axes, baselineAxes);
            finiteAxes = finiteAxes && AllFinite(axes);
            Thread.Sleep(4);
        }

        Console.WriteLine(
            "CAPYIO_RAW_GAMEPAD_RESULT=samples={0} timestamp_advances={1} button_changed={2} switch_changed={3} axis_changed={4} finite_axes={5}",
            samples,
            timestampAdvances,
            Lower(buttonChanged),
            Lower(switchChanged),
            Lower(axisChanged),
            Lower(finiteAxes));
        if (timestampAdvances == 0)
        {
            throw new InvalidOperationException("RawGameController report timestamp did not advance");
        }
        if (!finiteAxes)
        {
            throw new InvalidOperationException("RawGameController produced a non-finite axis value");
        }
        if (!buttonChanged && !switchChanged && !axisChanged)
        {
            throw new InvalidOperationException("no button, switch or axis changed during the probe");
        }
        Console.WriteLine("CAPYIO_RAW_GAMEPAD_REPORTS_PASSED");
        return 0;
    }

    private static RawGameController DiscoverController(
        ushort vendorId,
        ushort productId,
        out int finalMatches,
        out int finalInventoryCount,
        out int snapshots)
    {
        // Microsoft documents that RawGameControllers is initially empty even
        // when controllers are already connected. Observe the complete bounded
        // window before treating an empty or ambiguous inventory as evidence.
        DateTime deadline = DateTime.UtcNow.AddMilliseconds(DiscoveryMilliseconds);
        RawGameController target = null;
        finalMatches = 0;
        finalInventoryCount = 0;
        snapshots = 0;
        do
        {
            target = null;
            finalMatches = 0;
            finalInventoryCount = 0;
            foreach (RawGameController controller in RawGameController.RawGameControllers)
            {
                finalInventoryCount++;
                if (IsTarget(
                    controller.HardwareVendorId,
                    controller.HardwareProductId,
                    vendorId,
                    productId))
                {
                    target = controller;
                    finalMatches++;
                }
            }
            snapshots++;
            if (DateTime.UtcNow < deadline)
            {
                Thread.Sleep(DiscoveryPollMilliseconds);
            }
        }
        while (DateTime.UtcNow < deadline);
        return target;
    }

    private static int SelfTest()
    {
        if (ParseHex("054c", "vendor ID") != 0x054c
            || !IsTarget(0x054c, 0x09cc, 0x054c, 0x09cc)
            || IsTarget(0x054c, 0x09cc, 0x045e, 0x028e)
            || !Different(new bool[] { false, true }, new bool[] { false, false })
            || Different(new bool[] { false }, new bool[] { false })
            || !Different(
                new GameControllerSwitchPosition[] { GameControllerSwitchPosition.Up },
                new GameControllerSwitchPosition[] { GameControllerSwitchPosition.Center })
            || !Different(new double[] { 0.5, 0.25 }, new double[] { 0.5, 0.0 })
            || Different(new double[] { 0.5 }, new double[] { 0.5000001 })
            || !AllFinite(new double[] { 0.0, 1.0 })
            || AllFinite(new double[] { Double.NaN }))
        {
            throw new InvalidOperationException("RawGameController probe self-test failed");
        }
        Console.WriteLine("CAPYIO_RAW_GAMEPAD_SELF_TEST_PASSED");
        return 0;
    }

    private static bool IsTarget(
        ushort candidateVendorId,
        ushort candidateProductId,
        ushort vendorId,
        ushort productId)
    {
        return candidateVendorId == vendorId && candidateProductId == productId;
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

    private static bool Different(bool[] current, bool[] baseline)
    {
        for (int index = 0; index < current.Length; index++)
        {
            if (current[index] != baseline[index])
            {
                return true;
            }
        }
        return false;
    }

    private static bool Different(
        GameControllerSwitchPosition[] current,
        GameControllerSwitchPosition[] baseline)
    {
        for (int index = 0; index < current.Length; index++)
        {
            if (current[index] != baseline[index])
            {
                return true;
            }
        }
        return false;
    }

    private static bool Different(double[] current, double[] baseline)
    {
        const double epsilon = 0.000001;
        for (int index = 0; index < current.Length; index++)
        {
            if (Math.Abs(current[index] - baseline[index]) > epsilon)
            {
                return true;
            }
        }
        return false;
    }

    private static bool AllFinite(double[] values)
    {
        foreach (double value in values)
        {
            if (Double.IsNaN(value) || Double.IsInfinity(value))
            {
                return false;
            }
        }
        return true;
    }

    private static string Lower(bool value)
    {
        return value ? "true" : "false";
    }
}
