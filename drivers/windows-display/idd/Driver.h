#pragma once

#define NOMINMAX
#include <windows.h>
#include <bugcodes.h>
#include <wudfwdm.h>
#include <wdf.h>
#include <iddcx.h>

#include <dxgi1_5.h>
#include <d3d11_2.h>
#include <avrt.h>
#include <wrl.h>

#include <memory>
#include <array>
#include <mutex>
#include <vector>

#include "Trace.h"
#include "CapyIoDisplayControl.h"
#include "CapyIoDisplayHandoff.h"
#include "CapyIoDisplayProducer.h"

namespace Microsoft
{
    namespace WRL
    {
        namespace Wrappers
        {
            // Adds a wrapper for thread handles to the existing set of WRL handle wrapper classes
            typedef HandleT<HandleTraits::HANDLENullTraits> Thread;
        }
    }
}

namespace Microsoft
{
    namespace IndirectDisp
    {
        /// <summary>
        /// Manages the creation and lifetime of a Direct3D render device.
        /// </summary>
        struct IndirectSampleMonitor
        {
            static constexpr size_t szEdidBlock = 128;
            static constexpr size_t szModeList = 3;

            const BYTE pEdidBlock[szEdidBlock];
            const struct SampleMonitorMode {
                DWORD Width;
                DWORD Height;
                DWORD VSync;
            } pModeList[szModeList];
            const DWORD ulPreferredModeIdx;
        };

        /// <summary>
        /// Manages the creation and lifetime of a Direct3D render device.
        /// </summary>
        struct Direct3DDevice
        {
            Direct3DDevice(LUID AdapterLuid);
            Direct3DDevice();
            HRESULT Init();

            LUID AdapterLuid;
            Microsoft::WRL::ComPtr<IDXGIFactory5> DxgiFactory;
            Microsoft::WRL::ComPtr<IDXGIAdapter1> Adapter;
            Microsoft::WRL::ComPtr<ID3D11Device> Device;
            Microsoft::WRL::ComPtr<ID3D11DeviceContext> DeviceContext;
        };

        /// <summary>
        /// Manages a thread that consumes buffers from an indirect display swap-chain object.
        /// </summary>
        class SwapChainProcessor
        {
        public:
            SwapChainProcessor(
                IDDCX_SWAPCHAIN hSwapChain,
                std::shared_ptr<Direct3DDevice> Device,
                HANDLE NewFrameEvent,
                const std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes>& MonitorIdentity);
            ~SwapChainProcessor();

        private:
            static DWORD CALLBACK RunThread(LPVOID Argument);

            void Run();
            void RunCore();

            IDDCX_SWAPCHAIN m_hSwapChain;
            std::shared_ptr<Direct3DDevice> m_Device;
            HANDLE m_hAvailableBufferEvent;
            Microsoft::WRL::Wrappers::Thread m_hThread;
            Microsoft::WRL::Wrappers::Event m_hTerminateEvent;
            std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes> m_MonitorIdentity;
        };

        /// <summary>
        /// Provides a sample implementation of an indirect display driver.
        /// </summary>
        class IndirectDeviceContext
        {
        public:
            IndirectDeviceContext(_In_ WDFDEVICE WdfDevice);
            virtual ~IndirectDeviceContext();

            void InitAdapter();
            void CompleteAdapterInit(NTSTATUS Status) noexcept;
            NTSTATUS ConnectMonitor(
                const CapyIoDisplayControl::MonitorConnectRequest& Request,
                CapyIoDisplayControl::MonitorConnectResponse& Response);
            NTSTATUS DisconnectMonitor(const CapyIoDisplayControl::MonitorDisconnectRequest& Request);
            void QueryAdapter(CapyIoDisplayControl::AdapterQueryResponse& Response);
            void EnumerateMonitors(CapyIoDisplayControl::MonitorInventoryResponse& Response);

        private:
            struct MonitorSlot
            {
                bool Connected = false;
                std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes> Identity{};
                std::uint32_t PreferredModeIndex = 0;
                IDDCX_MONITOR Monitor = nullptr;
                LUID AdapterLuid{};
                std::uint32_t TargetId = 0;
            };

            WDFDEVICE m_WdfDevice;
            IDDCX_ADAPTER m_Adapter;
            bool m_AdapterReady = false;
            std::mutex m_MonitorMutex;
            std::array<MonitorSlot, CapyIoDisplayControl::MaxMonitors> m_MonitorSlots{};
        };

        class IndirectMonitorContext
        {
        public:
            IndirectMonitorContext(
                _In_ IDDCX_MONITOR Monitor,
                const std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes>& MonitorIdentity,
                std::uint32_t PreferredModeIndex);
            virtual ~IndirectMonitorContext();

            void AssignSwapChain(IDDCX_SWAPCHAIN SwapChain, LUID RenderAdapter, HANDLE NewFrameEvent);
            void UnassignSwapChain();
            std::uint32_t PreferredModeIndex() const noexcept;

        private:
            IDDCX_MONITOR m_Monitor;
            std::array<std::uint8_t, CapyIoDisplayHandoff::MonitorIdentityBytes> m_MonitorIdentity;
            std::uint32_t m_PreferredModeIndex;
            std::unique_ptr<SwapChainProcessor> m_ProcessingThread;
        } ;
    }
}
