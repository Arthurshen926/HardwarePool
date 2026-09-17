#pragma once

#include <cstdint>
#include <cstddef>

// Versioned memory layout shared with the user-mode CapyIO display Broker.
// Control-plane setup is separate; raw pixels never travel through IOCTL or
// JSON-RPC. All integer fields are little-endian on supported Windows targets.
namespace CapyIoDisplayHandoff
{
    static constexpr std::uint32_t Magic = 0x44595043; // "CPYD"
    static constexpr std::uint16_t AbiVersion = 1;
    static constexpr std::uint16_t HeaderBytes = 64;
    static constexpr std::uint32_t PixelFormatBgra8 = 1;
    static constexpr std::uint32_t MaxWidth = 4096;
    static constexpr std::uint32_t MaxHeight = 2160;
    static constexpr std::uint64_t MaxPayloadBytes = 64ull * 1024ull * 1024ull;
    static constexpr std::uint64_t TotalMappingBytes = HeaderBytes + MaxPayloadBytes;
    static constexpr wchar_t MappingName[] = L"Global\\CapyIO.DisplayFrame.v1";
    static constexpr wchar_t MappingNameV2Prefix[] = L"Global\\CapyIO.DisplayFrame.v2.";
    static constexpr std::size_t MonitorIdentityBytes = 16;
    static constexpr std::size_t MappingNameV2Chars =
        (sizeof(MappingNameV2Prefix) / sizeof(wchar_t)) - 1 + MonitorIdentityBytes * 2 + 1;

    struct alignas(8) FrameHeader
    {
        std::uint32_t MagicValue;
        std::uint16_t Version;
        std::uint16_t HeaderSize;
        std::uint64_t Generation;
        // Odd while the producer writes; even when the snapshot is stable.
        std::uint64_t SequenceLock;
        std::uint32_t Width;
        std::uint32_t Height;
        std::uint32_t StrideBytes;
        std::uint32_t PixelFormat;
        std::uint64_t PayloadBytes;
        std::uint64_t SourceTimestampNanos;
        std::uint32_t Flags;
        std::uint32_t Reserved;
    };

    static_assert(sizeof(FrameHeader) == HeaderBytes, "handoff ABI header size changed");
    static_assert(offsetof(FrameHeader, SequenceLock) % alignof(std::uint64_t) == 0,
        "sequence lock must remain naturally aligned");
}
