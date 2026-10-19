using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;

// Explicit physical-lab gate: creates only its own sidecar target, exercises
// XInput input/output, then requires removal. Never installs a driver.
internal static class FeedbackProbe
{
    [StructLayout(LayoutKind.Sequential)]
    struct State { public uint Packet; public ushort Buttons; public byte LT, RT; public short LX, LY, RX, RY; }
    [StructLayout(LayoutKind.Sequential)]
    struct Motors { public ushort Large, Small; }
    [DllImport("xinput1_4.dll")] static extern uint XInputGetState(uint index, out State state);
    [DllImport("xinput1_4.dll")] static extern uint XInputSetState(uint index, ref Motors motors);

    static int Main(string[] args)
    {
        try { Run(args); return 0; }
        catch (Exception error) { Console.Error.WriteLine(error.Message); return 2; }
    }

    static void Run(string[] args)
    {
        if (args.Length != 1 || !Path.IsPathRooted(args[0])) throw new ArgumentException("absolute sidecar path required");
        bool[] existing = new bool[4];
        State state;
        for (uint i = 0; i < 4; i++) existing[i] = XInputGetState(i, out state) == 0;
        ProcessStartInfo info = new ProcessStartInfo(args[0]) {
            UseShellExecute = false, CreateNoWindow = true,
            RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true
        };
        uint? owned = null;
        using (Process child = Process.Start(info)) {
            try {
                Task<string> ready = child.StandardOutput.ReadLineAsync();
                if (!ready.Wait(5000) || ready.Result != "CAPYIO_VIGEM_X360_SIDECAR_READY")
                    throw new IOException("sidecar not ready" + (child.HasExited ? ": " + child.StandardError.ReadToEnd() : ""));
                DateTime deadline = DateTime.UtcNow.AddSeconds(5);
                while (!owned.HasValue && DateTime.UtcNow < deadline) {
                    for (uint i = 0; i < 4; i++) if (!existing[i] && XInputGetState(i, out state) == 0) {
                        if (owned.HasValue) throw new IOException("ambiguous new XInput devices; no output sent");
                        owned = i;
                    }
                    Thread.Sleep(20);
                }
                if (!owned.HasValue) throw new IOException("new owned XInput slot not found");
                byte[] frame = new byte[20];
                frame[1] = 0x10; // A
                frame[4] = 127;
                frame[6] = 0x00; frame[7] = 0x40; // left X = 16384
                child.StandardInput.BaseStream.Write(frame, 0, frame.Length);
                child.StandardInput.BaseStream.Flush();
                bool input = false;
                deadline = DateTime.UtcNow.AddSeconds(2);
                while (DateTime.UtcNow < deadline) {
                    if (XInputGetState(owned.Value, out state) == 0 && state.Buttons == 0x1000 && state.LT == 127 && state.LX == 16384) { input = true; break; }
                    Thread.Sleep(10);
                }
                if (!input) throw new IOException("XInput complete-state input mismatch");
                Motors motors = new Motors { Large = 16384, Small = 32768 };
                Task<bool> feedback = Task.Run(() => {
                    Stream output = child.StandardOutput.BaseStream;
                    // Ignore initial all-zero driver records.
                    for (int attempt = 0; attempt < 64; attempt++) {
                        int small = output.ReadByte(), large = output.ReadByte();
                        if (small < 0 || large < 0) return false;
                        if (small >= 126 && small <= 128 && large >= 63 && large <= 64) return true;
                    }
                    return false;
                });
                if (XInputSetState(owned.Value, ref motors) != 0 || !feedback.Wait(3000) || !feedback.Result)
                    throw new IOException("XInput game output did not reach sidecar feedback");
                motors = new Motors();
                XInputSetState(owned.Value, ref motors);
                child.StandardInput.Close();
                if (!child.WaitForExit(3000) || child.ExitCode != 0) throw new IOException("sidecar did not stop cleanly");
                Console.WriteLine("XINPUT_INPUT_AND_GAME_FEEDBACK_PASSED");
            } finally {
                if (!child.HasExited) { child.Kill(); child.WaitForExit(); }
            }
        }
        if (owned.HasValue) {
            DateTime deadline = DateTime.UtcNow.AddSeconds(3);
            while (DateTime.UtcNow < deadline && XInputGetState(owned.Value, out state) == 0) Thread.Sleep(20);
            if (XInputGetState(owned.Value, out state) == 0) throw new IOException("owned XInput device remains after stop");
        }
        Console.WriteLine("XINPUT_OWNED_DEVICE_REMOVED");
    }
}
