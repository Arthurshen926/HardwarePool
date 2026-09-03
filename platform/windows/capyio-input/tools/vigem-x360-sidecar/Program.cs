using System;
using System.IO;
using Nefarius.ViGEm.Client;
using Nefarius.ViGEm.Client.Targets;

internal static class Program
{
    private const int FrameBytes = 20;

    private static int Main(string[] arguments)
    {
        try
        {
            if (arguments.Length == 1 && arguments[0] == "--self-test")
            {
                return SelfTest();
            }
            if (arguments.Length != 0)
            {
                throw new ArgumentException("usage: CapyIO.ViGEmX360Sidecar.exe [--self-test]");
            }
            return Run();
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("CAPYIO_VIGEM_X360_SIDECAR_FAILED: " + error.Message);
            return 2;
        }
    }

    private static int Run()
    {
        using (ViGEmClient client = new ViGEmClient())
        {
            IXbox360Controller controller = client.CreateXbox360Controller();
            controller.AutoSubmitReport = false;
            controller.Connect();
            try
            {
                Console.Out.WriteLine("CAPYIO_VIGEM_X360_SIDECAR_READY");
                Console.Out.Flush();

                Stream input = Console.OpenStandardInput();
                byte[] frame = new byte[FrameBytes];
                while (ReadFrame(input, frame))
                {
                    Submit(controller, frame);
                }
            }
            finally
            {
                controller.ResetReport();
                controller.SubmitReport();
                controller.Disconnect();
            }
        }
        return 0;
    }

    private static bool ReadFrame(Stream input, byte[] frame)
    {
        int offset = 0;
        while (offset < frame.Length)
        {
            int read = input.Read(frame, offset, frame.Length - offset);
            if (read == 0)
            {
                if (offset != 0)
                {
                    throw new EndOfStreamException("truncated 20-byte Xbox 360 state frame");
                }
                return false;
            }
            offset += read;
        }
        return true;
    }

    private static void Submit(IXbox360Controller controller, byte[] frame)
    {
        uint buttons = BitConverter.ToUInt32(frame, 0);
        if ((buttons & 0xffff0000U) != 0)
        {
            throw new InvalidDataException("Xbox 360 state contains unsupported button bits");
        }
        controller.SetButtonsFull((ushort)buttons);
        controller.SetSliderValue(0, frame[4]);
        controller.SetSliderValue(1, frame[5]);
        controller.SetAxisValue(0, BitConverter.ToInt16(frame, 6));
        controller.SetAxisValue(1, BitConverter.ToInt16(frame, 8));
        controller.SetAxisValue(2, BitConverter.ToInt16(frame, 10));
        controller.SetAxisValue(3, BitConverter.ToInt16(frame, 12));
        controller.SubmitReport();
    }

    private static int SelfTest()
    {
        byte[] frame = new byte[FrameBytes];
        frame[0] = 0x01;
        frame[1] = 0x10;
        frame[4] = 0x7f;
        frame[5] = 0xff;
        Array.Copy(BitConverter.GetBytes((short)-32768), 0, frame, 6, 2);
        Array.Copy(BitConverter.GetBytes((short)32767), 0, frame, 8, 2);
        if (BitConverter.ToUInt32(frame, 0) != 0x1001U
            || frame[4] != 0x7f
            || frame[5] != 0xff
            || BitConverter.ToInt16(frame, 6) != -32768
            || BitConverter.ToInt16(frame, 8) != 32767)
        {
            throw new InvalidOperationException("fixed Xbox 360 frame parsing self-test failed");
        }
        Console.Out.WriteLine("CAPYIO_VIGEM_X360_SIDECAR_SELF_TEST_PASSED");
        return 0;
    }
}
