#pragma once

#include <cstddef>
#include <cstdint>
#include <windows.h>

// Versioned, METHOD_BUFFERED control ABI shared with the trusted user-mode
// display controller. It carries monitor lifecycle metadata only; pixels,
// credentials, codec data and network configuration are forbidden here.
namespace CapyIoDisplayControl
{
    static constexpr std::uint32_t Magic = 0x43445043; // "CPDC"
    static constexpr std::uint16_t AbiVersion = 1;
    static constexpr std::uint32_t MaxMonitors = 8;
    static constexpr std::uint32_t MaxWidth = 4096;
    static constexpr std::uint32_t MaxHeight = 2160;
    static constexpr std::uint32_t MaxRefreshMilliHz = 240000;
    // {4F7BC2F1-940B-4D82-81D8-7E83BFA5E65C}
    static constexpr GUID DeviceInterfaceGuid =
        { 0x4f7bc2f1, 0x940b, 0x4d82, { 0x81, 0xd8, 0x7e, 0x83, 0xbf, 0xa5, 0xe6, 0x5c } };

    // Custom functions use read+write access so an arbitrary handle opened
    // without both rights cannot mutate monitor lifecycle.
    static constexpr ULONG IoctlQueryAdapter =
        CTL_CODE(FILE_DEVICE_UNKNOWN, 0x900, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA);
    static constexpr ULONG IoctlConnectMonitor =
        CTL_CODE(FILE_DEVICE_UNKNOWN, 0x901, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA);
    static constexpr ULONG IoctlDisconnectMonitor =
        CTL_CODE(FILE_DEVICE_UNKNOWN, 0x902, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA);
    static constexpr ULONG IoctlEnumerateMonitors =
        CTL_CODE(FILE_DEVICE_UNKNOWN, 0x903, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA);

#pragma pack(push, 1)
    struct MonitorConnectRequest
    {
        std::uint32_t MagicValue;
        std::uint16_t Version;
        std::uint16_t Size;
        // RFC 4122 byte order, matching CapabilityId::as_uuid().as_bytes().
        std::uint8_t MonitorIdentity[16];
        std::uint32_t Width;
        std::uint32_t Height;
        std::uint32_t RefreshMilliHz;
        std::uint32_t Flags;
        std::uint32_t Reserved[2];
    };

    struct MonitorDisconnectRequest
    {
        std::uint32_t MagicValue;
        std::uint16_t Version;
        std::uint16_t Size;
        std::uint8_t MonitorIdentity[16];
        std::uint32_t Flags;
        std::uint32_t Reserved;
    };

    struct AdapterQueryResponse
    {
        std::uint32_t MagicValue;
        std::uint16_t Version;
        std::uint16_t Size;
        std::uint32_t MaximumMonitors;
        std::uint32_t ConnectedMonitors;
        std::uint32_t Flags;
        std::uint32_t Reserved[3];
    };

    struct MonitorConnectResponse
    {
        std::uint32_t MagicValue;
        std::uint16_t Version;
        std::uint16_t Size;
        std::uint8_t MonitorIdentity[16];
        std::uint32_t ConnectorIndex;
        std::uint32_t TargetId;
        std::uint32_t AdapterLuidLowPart;
        std::int32_t AdapterLuidHighPart;
        std::uint32_t Reserved[2];
    };

    struct ConnectedMonitorEntry
    {
        // A zero identity marks an unused entry; every other field must then
        // also be zero. Connected entries use RFC 4122 identity byte order.
        std::uint8_t MonitorIdentity[16];
        std::uint32_t ConnectorIndex;
        std::uint32_t TargetId;
        std::uint32_t AdapterLuidLowPart;
        std::int32_t AdapterLuidHighPart;
    };

    struct MonitorInventoryResponse
    {
        std::uint32_t MagicValue;
        std::uint16_t Version;
        std::uint16_t Size;
        std::uint32_t ConnectedMonitors;
        std::uint32_t Flags;
        ConnectedMonitorEntry Entries[MaxMonitors];
    };
#pragma pack(pop)

    static_assert(sizeof(MonitorConnectRequest) == 48, "connect request ABI drift");
    static_assert(sizeof(MonitorDisconnectRequest) == 32, "disconnect request ABI drift");
    static_assert(sizeof(AdapterQueryResponse) == 32, "query response ABI drift");
    static_assert(sizeof(MonitorConnectResponse) == 48, "connect response ABI drift");
    static_assert(sizeof(ConnectedMonitorEntry) == 32, "inventory entry ABI drift");
    static_assert(sizeof(MonitorInventoryResponse) == 272, "inventory response ABI drift");
}
