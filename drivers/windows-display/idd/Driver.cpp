/*++

Copyright (c) Microsoft Corporation

Abstract:

    This module contains a sample implementation of an indirect display driver. See the included README.md file and the
    various TODO blocks throughout this file and all accompanying files for information on building a production driver.

    MSDN documentation on indirect displays can be found at https://msdn.microsoft.com/en-us/library/windows/hardware/mt761968(v=vs.85).aspx.

Environment:

    User Mode, UMDF

--*/

#include "Driver.h"
#include "Driver.tmh"

using namespace std;
using namespace Microsoft::IndirectDisp;
using namespace Microsoft::WRL;

#pragma region SampleMonitors

// Default modes reported for edid-less monitors. The first mode is set as preferred
static const struct IndirectSampleMonitor::SampleMonitorMode s_SampleDefaultModes[] =
{
    // The approved V2419A viewport is 2640x1216 (165:76). This lower-bandwidth
    // mode preserves that exact aspect ratio and requires AVC Level 4 at 30 fps.
    { 1980,  912, 60 },
    { 1920, 1080, 60 },
    { 1280,  720, 60 },
    { 1024,  768, 60 },
};

// FOR SAMPLE PURPOSES ONLY, Static info about monitors that will be reported to OS
static const struct IndirectSampleMonitor s_SampleMonitors[] =
{
    // Modified EDID from Dell S2719DGF
    {
        {
            0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0x10,0xAC,0xE6,0xD0,0x55,0x5A,0x4A,0x30,0x24,0x1D,0x01,
            0x04,0xA5,0x3C,0x22,0x78,0xFB,0x6C,0xE5,0xA5,0x55,0x50,0xA0,0x23,0x0B,0x50,0x54,0x00,0x02,0x00,
            0xD1,0xC0,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x58,0xE3,0x00,
            0xA0,0xA0,0xA0,0x29,0x50,0x30,0x20,0x35,0x00,0x55,0x50,0x21,0x00,0x00,0x1A,0x00,0x00,0x00,0xFF,
            0x00,0x37,0x4A,0x51,0x58,0x42,0x59,0x32,0x0A,0x20,0x20,0x20,0x20,0x20,0x00,0x00,0x00,0xFC,0x00,
            0x53,0x32,0x37,0x31,0x39,0x44,0x47,0x46,0x0A,0x20,0x20,0x20,0x20,0x00,0x00,0x00,0xFD,0x00,0x28,
            0x9B,0xFA,0xFA,0x40,0x01,0x0A,0x20,0x20,0x20,0x20,0x20,0x20,0x00,0x2C
        },
        {
            { 2560, 1440, 144 },
            { 1920, 1080,  60 },
            { 1024,  768,  60 },
        },
        0
    },
    // Modified EDID from Lenovo Y27fA
    {
        {
            0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0x30,0xAE,0xBF,0x65,0x01,0x01,0x01,0x01,0x20,0x1A,0x01,
            0x04,0xA5,0x3C,0x22,0x78,0x3B,0xEE,0xD1,0xA5,0x55,0x48,0x9B,0x26,0x12,0x50,0x54,0x00,0x08,0x00,
            0xA9,0xC0,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x68,0xD8,0x00,
            0x18,0xF1,0x70,0x2D,0x80,0x58,0x2C,0x45,0x00,0x53,0x50,0x21,0x00,0x00,0x1E,0x00,0x00,0x00,0x10,
            0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xFD,0x00,
            0x30,0x92,0xB4,0xB4,0x22,0x01,0x0A,0x20,0x20,0x20,0x20,0x20,0x20,0x00,0x00,0x00,0xFC,0x00,0x4C,
            0x45,0x4E,0x20,0x59,0x32,0x37,0x66,0x41,0x0A,0x20,0x20,0x20,0x00,0x11
        },
        {
            { 3840, 2160,  60 },
            { 1600,  900,  60 },
            { 1024,  768,  60 },
        },
        0
    }
};

#pragma endregion

#pragma region helpers

static inline void FillSignalInfo(DISPLAYCONFIG_VIDEO_SIGNAL_INFO& Mode, DWORD Width, DWORD Height, DWORD VSync, bool bMonitorMode)
{
    Mode.totalSize.cx = Mode.activeSize.cx = Width;
    Mode.totalSize.cy = Mode.activeSize.cy = Height;

    // See https://docs.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_video_signal_info
    Mode.AdditionalSignalInfo.vSyncFreqDivider = bMonitorMode ? 0 : 1;
    Mode.AdditionalSignalInfo.videoStandard = 255;

    Mode.vSyncFreq.Numerator = VSync;
    Mode.vSyncFreq.Denominator = 1;
    Mode.hSyncFreq.Numerator = VSync * Height;
    Mode.hSyncFreq.Denominator = 1;

    Mode.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;

    Mode.pixelRate = ((UINT64) VSync) * ((UINT64) Width) * ((UINT64) Height);
}

static IDDCX_MONITOR_MODE CreateIddCxMonitorMode(DWORD Width, DWORD Height, DWORD VSync, IDDCX_MONITOR_MODE_ORIGIN Origin = IDDCX_MONITOR_MODE_ORIGIN_DRIVER)
{
    IDDCX_MONITOR_MODE Mode = {};

    Mode.Size = sizeof(Mode);
    Mode.Origin = Origin;
    FillSignalInfo(Mode.MonitorVideoSignalInfo, Width, Height, VSync, true);

    return Mode;
}

static IDDCX_TARGET_MODE CreateIddCxTargetMode(DWORD Width, DWORD Height, DWORD VSync)
{
    IDDCX_TARGET_MODE Mode = {};

    Mode.Size = sizeof(Mode);
    FillSignalInfo(Mode.TargetVideoSignalInfo.targetVideoSignalInfo, Width, Height, VSync, false);

    return Mode;
}

static bool IsZeroIdentity(const std::uint8_t* Identity)
{
    std::uint8_t combined = 0;
    for (std::size_t index = 0; index < CapyIoDisplayHandoff::MonitorIdentityBytes; ++index)
    {
        combined |= Identity[index];
    }
    return combined == 0;
}

static bool IdentityEquals(
    const std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes>& Left,
    const std::uint8_t* Right)
{
    return memcmp(Left.data(), Right, Left.size()) == 0;
}

static GUID IdentityToGuid(const std::uint8_t* Identity)
{
    GUID value = {};
    value.Data1 =
        (static_cast<std::uint32_t>(Identity[0]) << 24) |
        (static_cast<std::uint32_t>(Identity[1]) << 16) |
        (static_cast<std::uint32_t>(Identity[2]) << 8) |
        static_cast<std::uint32_t>(Identity[3]);
    value.Data2 = static_cast<std::uint16_t>(
        (static_cast<std::uint16_t>(Identity[4]) << 8) | Identity[5]);
    value.Data3 = static_cast<std::uint16_t>(
        (static_cast<std::uint16_t>(Identity[6]) << 8) | Identity[7]);
    CopyMemory(value.Data4, Identity + 8, sizeof(value.Data4));
    return value;
}

static bool TryFindModeIndex(
    std::uint32_t Width,
    std::uint32_t Height,
    std::uint32_t RefreshMilliHz,
    std::uint32_t& ModeIndex)
{
    for (std::uint32_t index = 0; index < ARRAYSIZE(s_SampleDefaultModes); ++index)
    {
        const auto& mode = s_SampleDefaultModes[index];
        if (mode.Width == Width && mode.Height == Height && mode.VSync * 1000 == RefreshMilliHz)
        {
            ModeIndex = index;
            return true;
        }
    }
    return false;
}

#pragma endregion

extern "C" DRIVER_INITIALIZE DriverEntry;

EVT_WDF_DRIVER_DEVICE_ADD IddSampleDeviceAdd;
EVT_WDF_DEVICE_D0_ENTRY IddSampleDeviceD0Entry;
EVT_IDD_CX_DEVICE_IO_CONTROL IddSampleIoDeviceControl;

EVT_IDD_CX_ADAPTER_INIT_FINISHED IddSampleAdapterInitFinished;
EVT_IDD_CX_ADAPTER_COMMIT_MODES IddSampleAdapterCommitModes;

EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION IddSampleParseMonitorDescription;
EVT_IDD_CX_MONITOR_GET_DEFAULT_DESCRIPTION_MODES IddSampleMonitorGetDefaultModes;
EVT_IDD_CX_MONITOR_QUERY_TARGET_MODES IddSampleMonitorQueryModes;

EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN IddSampleMonitorAssignSwapChain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN IddSampleMonitorUnassignSwapChain;

struct IndirectDeviceContextWrapper
{
    IndirectDeviceContext* pContext;

    void Cleanup()
    {
        delete pContext;
        pContext = nullptr;
    }
};

struct IndirectMonitorContextWrapper
{
    IndirectMonitorContext* pContext;

    void Cleanup()
    {
        delete pContext;
        pContext = nullptr;
    }
};

// This macro creates the methods for accessing an IndirectDeviceContextWrapper as a context for a WDF object
WDF_DECLARE_CONTEXT_TYPE(IndirectDeviceContextWrapper);

WDF_DECLARE_CONTEXT_TYPE(IndirectMonitorContextWrapper);

extern "C" BOOL WINAPI DllMain(
    _In_ HINSTANCE hInstance,
    _In_ UINT dwReason,
    _In_opt_ LPVOID lpReserved)
{
    UNREFERENCED_PARAMETER(hInstance);
    UNREFERENCED_PARAMETER(lpReserved);
    UNREFERENCED_PARAMETER(dwReason);

    return TRUE;
}

_Use_decl_annotations_
extern "C" NTSTATUS DriverEntry(
    PDRIVER_OBJECT  pDriverObject,
    PUNICODE_STRING pRegistryPath
)
{
    WDF_DRIVER_CONFIG Config;
    NTSTATUS Status;

    WDF_OBJECT_ATTRIBUTES Attributes;
    WDF_OBJECT_ATTRIBUTES_INIT(&Attributes);

    WDF_DRIVER_CONFIG_INIT(&Config,
        IddSampleDeviceAdd
    );

    Status = WdfDriverCreate(pDriverObject, pRegistryPath, &Attributes, &Config, WDF_NO_HANDLE);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    return Status;
}

_Use_decl_annotations_
NTSTATUS IddSampleDeviceAdd(WDFDRIVER Driver, PWDFDEVICE_INIT pDeviceInit)
{
    NTSTATUS Status = STATUS_SUCCESS;
    WDF_PNPPOWER_EVENT_CALLBACKS PnpPowerCallbacks;

    UNREFERENCED_PARAMETER(Driver);

    // Register for power callbacks - in this sample only power-on is needed
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&PnpPowerCallbacks);
    PnpPowerCallbacks.EvtDeviceD0Entry = IddSampleDeviceD0Entry;
    WdfDeviceInitSetPnpPowerEventCallbacks(pDeviceInit, &PnpPowerCallbacks);

    IDD_CX_CLIENT_CONFIG IddConfig;
    IDD_CX_CLIENT_CONFIG_INIT(&IddConfig);

    IddConfig.EvtIddCxDeviceIoControl = IddSampleIoDeviceControl;

    IddConfig.EvtIddCxAdapterInitFinished = IddSampleAdapterInitFinished;

    IddConfig.EvtIddCxParseMonitorDescription = IddSampleParseMonitorDescription;
    IddConfig.EvtIddCxMonitorGetDefaultDescriptionModes = IddSampleMonitorGetDefaultModes;
    IddConfig.EvtIddCxMonitorQueryTargetModes = IddSampleMonitorQueryModes;
    IddConfig.EvtIddCxAdapterCommitModes = IddSampleAdapterCommitModes;
    IddConfig.EvtIddCxMonitorAssignSwapChain = IddSampleMonitorAssignSwapChain;
    IddConfig.EvtIddCxMonitorUnassignSwapChain = IddSampleMonitorUnassignSwapChain;

    Status = IddCxDeviceInitConfig(pDeviceInit, &IddConfig);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectDeviceContextWrapper);
    Attr.EvtCleanupCallback = [](WDFOBJECT Object)
    {
        // Automatically cleanup the context when the WDF object is about to be deleted
        auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(Object);
        if (pContext)
        {
            pContext->Cleanup();
        }
    };

    WDFDEVICE Device = nullptr;
    Status = WdfDeviceCreate(&pDeviceInit, &Attr, &Device);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    Status = WdfDeviceCreateDeviceInterface(
        Device, &CapyIoDisplayControl::DeviceInterfaceGuid, nullptr);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    Status = IddCxDeviceInitialize(Device);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    // Create a new device context object and attach it to the WDF device object
    auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(Device);
    pContext->pContext = new IndirectDeviceContext(Device);

    return Status;
}

_Use_decl_annotations_
NTSTATUS IddSampleDeviceD0Entry(WDFDEVICE Device, WDF_POWER_DEVICE_STATE PreviousState)
{
    UNREFERENCED_PARAMETER(PreviousState);

    // This function is called by WDF to start the device in the fully-on power state.

    auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(Device);
    pContext->pContext->InitAdapter();

    return STATUS_SUCCESS;
}

#pragma region Direct3DDevice

Direct3DDevice::Direct3DDevice(LUID AdapterLuid) : AdapterLuid(AdapterLuid)
{

}

Direct3DDevice::Direct3DDevice()
{
    AdapterLuid = LUID{};
}

HRESULT Direct3DDevice::Init()
{
    // The DXGI factory could be cached, but if a new render adapter appears on the system, a new factory needs to be
    // created. If caching is desired, check DxgiFactory->IsCurrent() each time and recreate the factory if !IsCurrent.
    HRESULT hr = CreateDXGIFactory2(0, IID_PPV_ARGS(&DxgiFactory));
    if (FAILED(hr))
    {
        return hr;
    }

    // Find the specified render adapter
    hr = DxgiFactory->EnumAdapterByLuid(AdapterLuid, IID_PPV_ARGS(&Adapter));
    if (FAILED(hr))
    {
        return hr;
    }

    // Create a D3D device using the render adapter. BGRA support is required by the WHQL test suite.
    hr = D3D11CreateDevice(Adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, &Device, nullptr, &DeviceContext);
    if (FAILED(hr))
    {
        // If creating the D3D device failed, it's possible the render GPU was lost (e.g. detachable GPU) or else the
        // system is in a transient state.
        return hr;
    }

    return S_OK;
}

#pragma endregion

#pragma region SwapChainProcessor

SwapChainProcessor::SwapChainProcessor(
    IDDCX_SWAPCHAIN hSwapChain,
    shared_ptr<Direct3DDevice> Device,
    HANDLE NewFrameEvent,
    const std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes>& MonitorIdentity)
    : m_hSwapChain(hSwapChain),
      m_Device(Device),
      m_hAvailableBufferEvent(NewFrameEvent),
      m_MonitorIdentity(MonitorIdentity)
{
    m_hTerminateEvent.Attach(CreateEvent(nullptr, FALSE, FALSE, nullptr));

    // Immediately create and run the swap-chain processing thread, passing 'this' as the thread parameter
    m_hThread.Attach(CreateThread(nullptr, 0, RunThread, this, 0, nullptr));
}

SwapChainProcessor::~SwapChainProcessor()
{
    // Alert the swap-chain processing thread to terminate
    SetEvent(m_hTerminateEvent.Get());

    if (m_hThread.Get())
    {
        // Wait for the thread to terminate
        WaitForSingleObject(m_hThread.Get(), INFINITE);
    }
}

DWORD CALLBACK SwapChainProcessor::RunThread(LPVOID Argument)
{
    reinterpret_cast<SwapChainProcessor*>(Argument)->Run();
    return 0;
}

void SwapChainProcessor::Run()
{
    // For improved performance, make use of the Multimedia Class Scheduler Service, which will intelligently
    // prioritize this thread for improved throughput in high CPU-load scenarios.
    DWORD AvTask = 0;
    HANDLE AvTaskHandle = AvSetMmThreadCharacteristicsW(L"Distribution", &AvTask);

    RunCore();

    // Always delete the swap-chain object when swap-chain processing loop terminates in order to kick the system to
    // provide a new swap-chain if necessary.
    WdfObjectDelete((WDFOBJECT)m_hSwapChain);
    m_hSwapChain = nullptr;

    if (AvTaskHandle != nullptr)
    {
        AvRevertMmThreadCharacteristics(AvTaskHandle);
    }
}

void SwapChainProcessor::RunCore()
{
    // Get the DXGI device interface
    ComPtr<IDXGIDevice> DxgiDevice;
    HRESULT hr = m_Device->Device.As(&DxgiDevice);
    if (FAILED(hr))
    {
        return;
    }

    IDARG_IN_SWAPCHAINSETDEVICE SetDevice = {};
    SetDevice.pDevice = DxgiDevice.Get();

    hr = IddCxSwapChainSetDevice(m_hSwapChain, &SetDevice);
    if (FAILED(hr))
    {
        return;
    }

    CapyIoDisplayHandoff::FrameProducer FrameProducer(m_MonitorIdentity);
    ComPtr<ID3D11Texture2D> StagingTexture;
    D3D11_TEXTURE2D_DESC StagingDescription = {};
    UINT AttachRetryCountdown = 0;

    // Acquire and release buffers in a loop
    for (;;)
    {
        ComPtr<IDXGIResource> AcquiredBuffer;

        // Ask for the next buffer from the producer
        IDARG_OUT_RELEASEANDACQUIREBUFFER Buffer = {};
        hr = IddCxSwapChainReleaseAndAcquireBuffer(m_hSwapChain, &Buffer);

        // AcquireBuffer immediately returns STATUS_PENDING if no buffer is yet available
        if (hr == E_PENDING)
        {
            // We must wait for a new buffer
            HANDLE WaitHandles [] =
            {
                m_hAvailableBufferEvent,
                m_hTerminateEvent.Get()
            };
            DWORD WaitResult = WaitForMultipleObjects(ARRAYSIZE(WaitHandles), WaitHandles, FALSE, 16);
            if (WaitResult == WAIT_OBJECT_0 || WaitResult == WAIT_TIMEOUT)
            {
                // We have a new buffer, so try the AcquireBuffer again
                continue;
            }
            else if (WaitResult == WAIT_OBJECT_0 + 1)
            {
                // We need to terminate
                break;
            }
            else
            {
                // The wait was cancelled or something unexpected happened
                hr = HRESULT_FROM_WIN32(WaitResult);
                break;
            }
        }
        else if (SUCCEEDED(hr))
        {
            // We have new frame to process, the surface has a reference on it that the driver has to release
            AcquiredBuffer.Attach(Buffer.MetaData.pSurface);

            // Keep the driver boundary local and bounded: copy the OS-selected
            // BGRA swapchain texture into one CPU-readable staging texture,
            // then publish only the newest packed frame to the Broker-owned
            // mapping. Encoding, authentication and networking stay outside.
            ComPtr<ID3D11Texture2D> SourceTexture;
            if (SUCCEEDED(AcquiredBuffer.As(&SourceTexture)))
            {
                D3D11_TEXTURE2D_DESC SourceDescription = {};
                SourceTexture->GetDesc(&SourceDescription);
                const bool SupportedFormat =
                    SourceDescription.Format == DXGI_FORMAT_B8G8R8A8_UNORM ||
                    SourceDescription.Format == DXGI_FORMAT_B8G8R8A8_UNORM_SRGB;
                const bool SupportedDimensions =
                    SourceDescription.Width > 0 &&
                    SourceDescription.Height > 0 &&
                    SourceDescription.Width <= CapyIoDisplayHandoff::MaxWidth &&
                    SourceDescription.Height <= CapyIoDisplayHandoff::MaxHeight;

                if (SupportedFormat && SupportedDimensions)
                {
                    const bool NeedsStagingTexture =
                        StagingTexture == nullptr ||
                        StagingDescription.Width != SourceDescription.Width ||
                        StagingDescription.Height != SourceDescription.Height ||
                        StagingDescription.Format != SourceDescription.Format;
                    if (NeedsStagingTexture)
                    {
                        StagingTexture.Reset();
                        StagingDescription = SourceDescription;
                        StagingDescription.Usage = D3D11_USAGE_STAGING;
                        StagingDescription.BindFlags = 0;
                        StagingDescription.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
                        StagingDescription.MiscFlags = 0;
                        StagingDescription.MipLevels = 1;
                        StagingDescription.ArraySize = 1;
                        StagingDescription.SampleDesc.Count = 1;
                        StagingDescription.SampleDesc.Quality = 0;
                        if (FAILED(m_Device->Device->CreateTexture2D(
                                &StagingDescription, nullptr, &StagingTexture)))
                        {
                            StagingTexture.Reset();
                        }
                    }

                    if (StagingTexture != nullptr)
                    {
                        m_Device->DeviceContext->CopyResource(
                            StagingTexture.Get(), SourceTexture.Get());
                        D3D11_MAPPED_SUBRESOURCE Mapped = {};
                        if (SUCCEEDED(m_Device->DeviceContext->Map(
                                StagingTexture.Get(), 0, D3D11_MAP_READ, 0, &Mapped)))
                        {
                            if (!FrameProducer.IsAttached())
                            {
                                if (AttachRetryCountdown == 0)
                                {
                                    FrameProducer.Attach();
                                    AttachRetryCountdown = 60;
                                }
                                else
                                {
                                    --AttachRetryCountdown;
                                }
                            }
                            if (FrameProducer.IsAttached())
                            {
                                const std::uint64_t TimestampNanos =
                                    static_cast<std::uint64_t>(GetTickCount64()) * 1'000'000ull;
                                FrameProducer.TryWrite(
                                    static_cast<const std::uint8_t*>(Mapped.pData),
                                    Mapped.RowPitch,
                                    SourceDescription.Width,
                                    SourceDescription.Height,
                                    TimestampNanos);
                            }
                            m_Device->DeviceContext->Unmap(StagingTexture.Get(), 0);
                        }
                    }
                }
            }

            // We have finished processing this frame hence we release the reference on it.
            // If the driver forgets to release the reference to the surface, it will be leaked which results in the
            // surfaces being left around after swapchain is destroyed.
            // NOTE: Although in this sample we release reference to the surface here; the driver still
            // owns the Buffer.MetaData.pSurface surface until IddCxSwapChainReleaseAndAcquireBuffer returns
            // S_OK and gives us a new frame, a driver may want to use the surface in future to re-encode the desktop
            // for better quality if there is no new frame for a while
            AcquiredBuffer.Reset();

            // Indicate to OS that we have finished inital processing of the frame, it is a hint that
            // OS could start preparing another frame
            hr = IddCxSwapChainFinishedProcessingFrame(m_hSwapChain);
            if (FAILED(hr))
            {
                break;
            }

            // ==============================
            // TODO: Report frame statistics once the asynchronous encode/send work is completed
            //
            // Drivers should report information about sub-frame timings, like encode time, send time, etc.
            // ==============================
            // IddCxSwapChainReportFrameStatistics(m_hSwapChain, ...);
        }
        else
        {
            // The swap-chain was likely abandoned (e.g. DXGI_ERROR_ACCESS_LOST), so exit the processing loop
            break;
        }
    }
}

#pragma endregion

#pragma region IndirectDeviceContext

IndirectDeviceContext::IndirectDeviceContext(_In_ WDFDEVICE WdfDevice) :
    m_WdfDevice(WdfDevice)
{
    m_Adapter = {};
}

IndirectDeviceContext::~IndirectDeviceContext()
{
}

void IndirectDeviceContext::InitAdapter()
{
    // ==============================
    // TODO: Update the below diagnostic information in accordance with the target hardware. The strings and version
    // numbers are used for telemetry and may be displayed to the user in some situations.
    //
    // This is also where static per-adapter capabilities are determined.
    // ==============================

    IDDCX_ADAPTER_CAPS AdapterCaps = {};
    AdapterCaps.Size = sizeof(AdapterCaps);

    // Declare basic feature support for the adapter (required)
    AdapterCaps.MaxMonitorsSupported = CapyIoDisplayControl::MaxMonitors;
    AdapterCaps.EndPointDiagnostics.Size = sizeof(AdapterCaps.EndPointDiagnostics);
    AdapterCaps.EndPointDiagnostics.GammaSupport = IDDCX_FEATURE_IMPLEMENTATION_NONE;
    AdapterCaps.EndPointDiagnostics.TransmissionType = IDDCX_TRANSMISSION_TYPE_WIRED_OTHER;

    // Declare your device strings for telemetry (required)
    AdapterCaps.EndPointDiagnostics.pEndPointFriendlyName = L"CapyIO Phone Display";
    AdapterCaps.EndPointDiagnostics.pEndPointManufacturerName = L"CapyIO";
    AdapterCaps.EndPointDiagnostics.pEndPointModelName = L"CapyIO Virtual Monitor";

    // Declare your hardware and firmware versions (required)
    IDDCX_ENDPOINT_VERSION Version = {};
    Version.Size = sizeof(Version);
    Version.MajorVer = 1;
    AdapterCaps.EndPointDiagnostics.pFirmwareVersion = &Version;
    AdapterCaps.EndPointDiagnostics.pHardwareVersion = &Version;

    // Initialize a WDF context that can store a pointer to the device context object
    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectDeviceContextWrapper);

    IDARG_IN_ADAPTER_INIT AdapterInit = {};
    AdapterInit.WdfDevice = m_WdfDevice;
    AdapterInit.pCaps = &AdapterCaps;
    AdapterInit.ObjectAttributes = &Attr;

    // Start the initialization of the adapter, which will trigger the AdapterFinishInit callback later
    IDARG_OUT_ADAPTER_INIT AdapterInitOut;
    NTSTATUS Status = IddCxAdapterInitAsync(&AdapterInit, &AdapterInitOut);

    if (NT_SUCCESS(Status))
    {
        // Store a reference to the WDF adapter handle
        m_Adapter = AdapterInitOut.AdapterObject;

        // Store the device context object into the WDF object context
        auto* pContext = WdfObjectGet_IndirectDeviceContextWrapper(AdapterInitOut.AdapterObject);
        pContext->pContext = this;
    }
}

void IndirectDeviceContext::CompleteAdapterInit(NTSTATUS Status) noexcept
{
    std::lock_guard<std::mutex> lock(m_MonitorMutex);
    m_AdapterReady = NT_SUCCESS(Status);
}

NTSTATUS IndirectDeviceContext::ConnectMonitor(
    const CapyIoDisplayControl::MonitorConnectRequest& Request,
    CapyIoDisplayControl::MonitorConnectResponse& Response)
{
    std::lock_guard<std::mutex> lock(m_MonitorMutex);
    if (!m_AdapterReady || m_Adapter == nullptr)
    {
        return STATUS_DEVICE_NOT_READY;
    }

    std::uint32_t preferredModeIndex = 0;
    if (!TryFindModeIndex(
            Request.Width, Request.Height, Request.RefreshMilliHz, preferredModeIndex))
    {
        return STATUS_NOT_SUPPORTED;
    }

    for (const auto& slot : m_MonitorSlots)
    {
        if (!slot.Connected || !IdentityEquals(slot.Identity, Request.MonitorIdentity))
        {
            continue;
        }
        if (slot.PreferredModeIndex != preferredModeIndex)
        {
            return STATUS_OBJECT_NAME_COLLISION;
        }
        Response = {};
        Response.MagicValue = CapyIoDisplayControl::Magic;
        Response.Version = CapyIoDisplayControl::AbiVersion;
        Response.Size = sizeof(Response);
        CopyMemory(Response.MonitorIdentity, slot.Identity.data(), slot.Identity.size());
        Response.ConnectorIndex = static_cast<std::uint32_t>(&slot - m_MonitorSlots.data());
        Response.TargetId = slot.TargetId;
        Response.AdapterLuidLowPart = slot.AdapterLuid.LowPart;
        Response.AdapterLuidHighPart = slot.AdapterLuid.HighPart;
        return STATUS_SUCCESS;
    }

    std::uint32_t connectorIndex = CapyIoDisplayControl::MaxMonitors;
    for (std::uint32_t index = 0; index < m_MonitorSlots.size(); ++index)
    {
        if (!m_MonitorSlots[index].Connected)
        {
            connectorIndex = index;
            break;
        }
    }
    if (connectorIndex == CapyIoDisplayControl::MaxMonitors)
    {
        return STATUS_TOO_MANY_NODES;
    }

    WDF_OBJECT_ATTRIBUTES Attr;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&Attr, IndirectMonitorContextWrapper);
    Attr.EvtCleanupCallback = [](WDFOBJECT Object)
    {
        auto* context = WdfObjectGet_IndirectMonitorContextWrapper(Object);
        if (context != nullptr)
        {
            context->Cleanup();
        }
    };

    IDDCX_MONITOR_INFO MonitorInfo = {};
    MonitorInfo.Size = sizeof(MonitorInfo);
    MonitorInfo.MonitorType = DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI;
    MonitorInfo.ConnectorIndex = connectorIndex;

    MonitorInfo.MonitorDescription.Size = sizeof(MonitorInfo.MonitorDescription);
    MonitorInfo.MonitorDescription.Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    MonitorInfo.MonitorDescription.DataSize = 0;
    MonitorInfo.MonitorDescription.pData = nullptr;
    MonitorInfo.MonitorContainerId = IdentityToGuid(Request.MonitorIdentity);

    IDARG_IN_MONITORCREATE MonitorCreate = {};
    MonitorCreate.ObjectAttributes = &Attr;
    MonitorCreate.pMonitorInfo = &MonitorInfo;

    // Create a monitor object with the specified monitor descriptor
    IDARG_OUT_MONITORCREATE MonitorCreateOut;
    NTSTATUS Status = IddCxMonitorCreate(m_Adapter, &MonitorCreate, &MonitorCreateOut);
    if (!NT_SUCCESS(Status))
    {
        return Status;
    }

    std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes> identity{};
    CopyMemory(identity.data(), Request.MonitorIdentity, identity.size());
    auto* monitorContextWrapper =
        WdfObjectGet_IndirectMonitorContextWrapper(MonitorCreateOut.MonitorObject);
    monitorContextWrapper->pContext = new IndirectMonitorContext(
        MonitorCreateOut.MonitorObject, identity, preferredModeIndex);

    IDARG_OUT_MONITORARRIVAL ArrivalOut = {};
    Status = IddCxMonitorArrival(MonitorCreateOut.MonitorObject, &ArrivalOut);
    if (!NT_SUCCESS(Status))
    {
        WdfObjectDelete(MonitorCreateOut.MonitorObject);
        return Status;
    }

    auto& slot = m_MonitorSlots[connectorIndex];
    slot.Connected = true;
    slot.Identity = identity;
    slot.PreferredModeIndex = preferredModeIndex;
    slot.Monitor = MonitorCreateOut.MonitorObject;
    slot.AdapterLuid = ArrivalOut.OsAdapterLuid;
    slot.TargetId = ArrivalOut.OsTargetId;

    Response = {};
    Response.MagicValue = CapyIoDisplayControl::Magic;
    Response.Version = CapyIoDisplayControl::AbiVersion;
    Response.Size = sizeof(Response);
    CopyMemory(Response.MonitorIdentity, identity.data(), identity.size());
    Response.ConnectorIndex = connectorIndex;
    Response.TargetId = slot.TargetId;
    Response.AdapterLuidLowPart = slot.AdapterLuid.LowPart;
    Response.AdapterLuidHighPart = slot.AdapterLuid.HighPart;
    return STATUS_SUCCESS;
}

NTSTATUS IndirectDeviceContext::DisconnectMonitor(
    const CapyIoDisplayControl::MonitorDisconnectRequest& Request)
{
    std::lock_guard<std::mutex> lock(m_MonitorMutex);
    if (!m_AdapterReady)
    {
        return STATUS_DEVICE_NOT_READY;
    }
    for (auto& slot : m_MonitorSlots)
    {
        if (!slot.Connected || !IdentityEquals(slot.Identity, Request.MonitorIdentity))
        {
            continue;
        }
        IddCxMonitorDeparture(slot.Monitor);
        slot = {};
        return STATUS_SUCCESS;
    }
    return STATUS_SUCCESS;
}

void IndirectDeviceContext::QueryAdapter(
    CapyIoDisplayControl::AdapterQueryResponse& Response)
{
    std::lock_guard<std::mutex> lock(m_MonitorMutex);
    Response = {};
    Response.MagicValue = CapyIoDisplayControl::Magic;
    Response.Version = CapyIoDisplayControl::AbiVersion;
    Response.Size = sizeof(Response);
    Response.MaximumMonitors = CapyIoDisplayControl::MaxMonitors;
    for (const auto& slot : m_MonitorSlots)
    {
        if (slot.Connected)
        {
            ++Response.ConnectedMonitors;
        }
    }
}

void IndirectDeviceContext::EnumerateMonitors(
    CapyIoDisplayControl::MonitorInventoryResponse& Response)
{
    std::lock_guard<std::mutex> lock(m_MonitorMutex);
    Response = {};
    Response.MagicValue = CapyIoDisplayControl::Magic;
    Response.Version = CapyIoDisplayControl::AbiVersion;
    Response.Size = sizeof(Response);
    for (std::uint32_t index = 0; index < m_MonitorSlots.size(); ++index)
    {
        const auto& slot = m_MonitorSlots[index];
        if (!slot.Connected)
        {
            continue;
        }
        auto& entry = Response.Entries[index];
        CopyMemory(entry.MonitorIdentity, slot.Identity.data(), slot.Identity.size());
        entry.ConnectorIndex = index;
        entry.TargetId = slot.TargetId;
        entry.AdapterLuidLowPart = slot.AdapterLuid.LowPart;
        entry.AdapterLuidHighPart = slot.AdapterLuid.HighPart;
        ++Response.ConnectedMonitors;
    }
}

IndirectMonitorContext::IndirectMonitorContext(
    _In_ IDDCX_MONITOR Monitor,
    const std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes>& MonitorIdentity,
    std::uint32_t PreferredModeIndex) :
    m_Monitor(Monitor),
    m_MonitorIdentity(MonitorIdentity),
    m_PreferredModeIndex(PreferredModeIndex)
{
}

IndirectMonitorContext::~IndirectMonitorContext()
{
    m_ProcessingThread.reset();
}

void IndirectMonitorContext::AssignSwapChain(IDDCX_SWAPCHAIN SwapChain, LUID RenderAdapter, HANDLE NewFrameEvent)
{
    m_ProcessingThread.reset();

    auto Device = make_shared<Direct3DDevice>(RenderAdapter);
    if (FAILED(Device->Init()))
    {
        // It's important to delete the swap-chain if D3D initialization fails, so that the OS knows to generate a new
        // swap-chain and try again.
        WdfObjectDelete(SwapChain);
    }
    else
    {
        // Create a new swap-chain processing thread
        m_ProcessingThread.reset(new SwapChainProcessor(
            SwapChain, Device, NewFrameEvent, m_MonitorIdentity));
    }
}

std::uint32_t IndirectMonitorContext::PreferredModeIndex() const noexcept
{
    return m_PreferredModeIndex;
}

void IndirectMonitorContext::UnassignSwapChain()
{
    // Stop processing the last swap-chain
    m_ProcessingThread.reset();
}

#pragma endregion

#pragma region DDI Callbacks

_Use_decl_annotations_
NTSTATUS IddSampleAdapterInitFinished(IDDCX_ADAPTER AdapterObject, const IDARG_IN_ADAPTER_INIT_FINISHED* pInArgs)
{
    // This is called when the OS has finished setting up the adapter for use by the IddCx driver. It's now possible
    // to report attached monitors.

    auto* pDeviceContextWrapper = WdfObjectGet_IndirectDeviceContextWrapper(AdapterObject);
    pDeviceContextWrapper->pContext->CompleteAdapterInit(pInArgs->AdapterInitStatus);

    return STATUS_SUCCESS;
}

_Use_decl_annotations_
void IddSampleIoDeviceControl(
    WDFDEVICE Device,
    WDFREQUEST Request,
    size_t OutputBufferLength,
    size_t InputBufferLength,
    ULONG IoControlCode)
{
    auto* deviceContextWrapper = WdfObjectGet_IndirectDeviceContextWrapper(Device);
    if (deviceContextWrapper == nullptr || deviceContextWrapper->pContext == nullptr)
    {
        WdfRequestComplete(Request, STATUS_DEVICE_NOT_READY);
        return;
    }

    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;
    size_t bytesReturned = 0;
    switch (IoControlCode)
    {
    case CapyIoDisplayControl::IoctlQueryAdapter:
    {
        if (InputBufferLength != 0 ||
            OutputBufferLength < sizeof(CapyIoDisplayControl::AdapterQueryResponse))
        {
            status = STATUS_INFO_LENGTH_MISMATCH;
            break;
        }
        CapyIoDisplayControl::AdapterQueryResponse* response = nullptr;
        status = WdfRequestRetrieveOutputBuffer(
            Request, sizeof(*response), reinterpret_cast<void**>(&response), nullptr);
        if (NT_SUCCESS(status))
        {
            deviceContextWrapper->pContext->QueryAdapter(*response);
            bytesReturned = sizeof(*response);
        }
        break;
    }
    case CapyIoDisplayControl::IoctlConnectMonitor:
    {
        if (InputBufferLength != sizeof(CapyIoDisplayControl::MonitorConnectRequest) ||
            OutputBufferLength < sizeof(CapyIoDisplayControl::MonitorConnectResponse))
        {
            status = STATUS_INFO_LENGTH_MISMATCH;
            break;
        }
        CapyIoDisplayControl::MonitorConnectRequest* input = nullptr;
        CapyIoDisplayControl::MonitorConnectResponse* response = nullptr;
        status = WdfRequestRetrieveInputBuffer(
            Request, sizeof(*input), reinterpret_cast<void**>(&input), nullptr);
        if (!NT_SUCCESS(status))
        {
            break;
        }
        status = WdfRequestRetrieveOutputBuffer(
            Request, sizeof(*response), reinterpret_cast<void**>(&response), nullptr);
        if (!NT_SUCCESS(status))
        {
            break;
        }
        if (input->MagicValue != CapyIoDisplayControl::Magic ||
            input->Version != CapyIoDisplayControl::AbiVersion ||
            input->Size != sizeof(*input) ||
            IsZeroIdentity(input->MonitorIdentity) ||
            input->Width == 0 || input->Width > CapyIoDisplayControl::MaxWidth ||
            input->Height == 0 || input->Height > CapyIoDisplayControl::MaxHeight ||
            input->RefreshMilliHz == 0 ||
            input->RefreshMilliHz > CapyIoDisplayControl::MaxRefreshMilliHz ||
            input->Flags != 0 || input->Reserved[0] != 0 || input->Reserved[1] != 0)
        {
            status = STATUS_INVALID_PARAMETER;
            break;
        }
        status = deviceContextWrapper->pContext->ConnectMonitor(*input, *response);
        if (NT_SUCCESS(status))
        {
            bytesReturned = sizeof(*response);
        }
        break;
    }
    case CapyIoDisplayControl::IoctlDisconnectMonitor:
    {
        if (InputBufferLength != sizeof(CapyIoDisplayControl::MonitorDisconnectRequest) ||
            OutputBufferLength != 0)
        {
            status = STATUS_INFO_LENGTH_MISMATCH;
            break;
        }
        CapyIoDisplayControl::MonitorDisconnectRequest* input = nullptr;
        status = WdfRequestRetrieveInputBuffer(
            Request, sizeof(*input), reinterpret_cast<void**>(&input), nullptr);
        if (!NT_SUCCESS(status))
        {
            break;
        }
        if (input->MagicValue != CapyIoDisplayControl::Magic ||
            input->Version != CapyIoDisplayControl::AbiVersion ||
            input->Size != sizeof(*input) ||
            IsZeroIdentity(input->MonitorIdentity) || input->Flags != 0 || input->Reserved != 0)
        {
            status = STATUS_INVALID_PARAMETER;
            break;
        }
        status = deviceContextWrapper->pContext->DisconnectMonitor(*input);
        break;
    }
    case CapyIoDisplayControl::IoctlEnumerateMonitors:
    {
        if (InputBufferLength != 0 ||
            OutputBufferLength < sizeof(CapyIoDisplayControl::MonitorInventoryResponse))
        {
            status = STATUS_INFO_LENGTH_MISMATCH;
            break;
        }
        CapyIoDisplayControl::MonitorInventoryResponse* response = nullptr;
        status = WdfRequestRetrieveOutputBuffer(
            Request, sizeof(*response), reinterpret_cast<void**>(&response), nullptr);
        if (NT_SUCCESS(status))
        {
            deviceContextWrapper->pContext->EnumerateMonitors(*response);
            bytesReturned = sizeof(*response);
        }
        break;
    }
    default:
        break;
    }

    WdfRequestCompleteWithInformation(Request, status, bytesReturned);
}

_Use_decl_annotations_
NTSTATUS IddSampleAdapterCommitModes(IDDCX_ADAPTER AdapterObject, const IDARG_IN_COMMITMODES* pInArgs)
{
    UNREFERENCED_PARAMETER(AdapterObject);
    UNREFERENCED_PARAMETER(pInArgs);

    // For the sample, do nothing when modes are picked - the swap-chain is taken care of by IddCx

    // ==============================
    // TODO: In a real driver, this function would be used to reconfigure the device to commit the new modes. Loop
    // through pInArgs->pPaths and look for IDDCX_PATH_FLAGS_ACTIVE. Any path not active is inactive (e.g. the monitor
    // should be turned off).
    // ==============================

    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS IddSampleParseMonitorDescription(const IDARG_IN_PARSEMONITORDESCRIPTION* pInArgs, IDARG_OUT_PARSEMONITORDESCRIPTION* pOutArgs)
{
    // ==============================
    // TODO: In a real driver, this function would be called to generate monitor modes for an EDID by parsing it. In
    // this sample driver, we hard-code the EDID, so this function can generate known modes.
    // ==============================

    pOutArgs->MonitorModeBufferOutputCount = IndirectSampleMonitor::szModeList;

    if (pInArgs->MonitorModeBufferInputCount < IndirectSampleMonitor::szModeList)
    {
        // Return success if there was no buffer, since the caller was only asking for a count of modes
        return (pInArgs->MonitorModeBufferInputCount > 0) ? STATUS_BUFFER_TOO_SMALL : STATUS_SUCCESS;
    }
    else
    {
        // In the sample driver, we have reported some static information about connected monitors
        // Check which of the reported monitors this call is for by comparing it to the pointer of
        // our known EDID blocks.

        if (pInArgs->MonitorDescription.DataSize != IndirectSampleMonitor::szEdidBlock)
            return STATUS_INVALID_PARAMETER;

        DWORD SampleMonitorIdx = 0;
        for(; SampleMonitorIdx < ARRAYSIZE(s_SampleMonitors); SampleMonitorIdx++)
        {
            if (memcmp(pInArgs->MonitorDescription.pData, s_SampleMonitors[SampleMonitorIdx].pEdidBlock, IndirectSampleMonitor::szEdidBlock) == 0)
            {
                // Copy the known modes to the output buffer
                for (DWORD ModeIndex = 0; ModeIndex < IndirectSampleMonitor::szModeList; ModeIndex++)
                {
                    pInArgs->pMonitorModes[ModeIndex] = CreateIddCxMonitorMode(
                        s_SampleMonitors[SampleMonitorIdx].pModeList[ModeIndex].Width,
                        s_SampleMonitors[SampleMonitorIdx].pModeList[ModeIndex].Height,
                        s_SampleMonitors[SampleMonitorIdx].pModeList[ModeIndex].VSync,
                        IDDCX_MONITOR_MODE_ORIGIN_MONITORDESCRIPTOR
                    );
                }

                // Set the preferred mode as represented in the EDID
                pOutArgs->PreferredMonitorModeIdx = s_SampleMonitors[SampleMonitorIdx].ulPreferredModeIdx;

                return STATUS_SUCCESS;
            }
        }

        // This EDID block does not belong to the monitors we reported earlier
        return STATUS_INVALID_PARAMETER;
    }
}

_Use_decl_annotations_
NTSTATUS IddSampleMonitorGetDefaultModes(IDDCX_MONITOR MonitorObject, const IDARG_IN_GETDEFAULTDESCRIPTIONMODES* pInArgs, IDARG_OUT_GETDEFAULTDESCRIPTIONMODES* pOutArgs)
{
    // ==============================
    // TODO: In a real driver, this function would be called to generate monitor modes for a monitor with no EDID.
    // Drivers should report modes that are guaranteed to be supported by the transport protocol and by nearly all
    // monitors (such 640x480, 800x600, or 1024x768). If the driver has access to monitor modes from a descriptor other
    // than an EDID, those modes would also be reported here.
    // ==============================

    pOutArgs->DefaultMonitorModeBufferOutputCount = ARRAYSIZE(s_SampleDefaultModes);
    if (pInArgs->DefaultMonitorModeBufferInputCount == 0)
    {
        return STATUS_SUCCESS;
    }
    if (pInArgs->DefaultMonitorModeBufferInputCount < ARRAYSIZE(s_SampleDefaultModes))
    {
        return STATUS_BUFFER_TOO_SMALL;
    }

    for (DWORD ModeIndex = 0; ModeIndex < ARRAYSIZE(s_SampleDefaultModes); ModeIndex++)
    {
        pInArgs->pDefaultMonitorModes[ModeIndex] = CreateIddCxMonitorMode(
            s_SampleDefaultModes[ModeIndex].Width,
            s_SampleDefaultModes[ModeIndex].Height,
            s_SampleDefaultModes[ModeIndex].VSync,
            IDDCX_MONITOR_MODE_ORIGIN_DRIVER
        );
    }

    auto* monitorContextWrapper = WdfObjectGet_IndirectMonitorContextWrapper(MonitorObject);
    pOutArgs->PreferredMonitorModeIdx =
        monitorContextWrapper->pContext->PreferredModeIndex();

    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS IddSampleMonitorQueryModes(IDDCX_MONITOR MonitorObject, const IDARG_IN_QUERYTARGETMODES* pInArgs, IDARG_OUT_QUERYTARGETMODES* pOutArgs)
{
    UNREFERENCED_PARAMETER(MonitorObject);

    vector<IDDCX_TARGET_MODE> TargetModes;

    // Create a set of modes supported for frame processing and scan-out. These are typically not based on the
    // monitor's descriptor and instead are based on the static processing capability of the device. The OS will
    // report the available set of modes for a given output as the intersection of monitor modes with target modes.

    TargetModes.push_back(CreateIddCxTargetMode(1980,  912, 60));
    TargetModes.push_back(CreateIddCxTargetMode(1920, 1080, 60));
    TargetModes.push_back(CreateIddCxTargetMode(1280,  720, 60));
    TargetModes.push_back(CreateIddCxTargetMode(1024,  768, 60));

    pOutArgs->TargetModeBufferOutputCount = (UINT) TargetModes.size();

    if (pInArgs->TargetModeBufferInputCount >= TargetModes.size())
    {
        copy(TargetModes.begin(), TargetModes.end(), pInArgs->pTargetModes);
    }

    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS IddSampleMonitorAssignSwapChain(IDDCX_MONITOR MonitorObject, const IDARG_IN_SETSWAPCHAIN* pInArgs)
{
    auto* pMonitorContextWrapper = WdfObjectGet_IndirectMonitorContextWrapper(MonitorObject);
    pMonitorContextWrapper->pContext->AssignSwapChain(pInArgs->hSwapChain, pInArgs->RenderAdapterLuid, pInArgs->hNextSurfaceAvailable);
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS IddSampleMonitorUnassignSwapChain(IDDCX_MONITOR MonitorObject)
{
    auto* pMonitorContextWrapper = WdfObjectGet_IndirectMonitorContextWrapper(MonitorObject);
    pMonitorContextWrapper->pContext->UnassignSwapChain();
    return STATUS_SUCCESS;
}

#pragma endregion
