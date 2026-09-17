#pragma once

#include "CapyIoDisplayHandoff.h"

#include <windows.h>
#include <array>
#include <cstdint>

namespace CapyIoDisplayHandoff
{
    class FrameProducer final
    {
    public:
        explicit FrameProducer(
            const std::array<std::uint8_t, MonitorIdentityBytes>& monitorIdentity) noexcept;
        ~FrameProducer() noexcept;

        FrameProducer(const FrameProducer&) = delete;
        FrameProducer& operator=(const FrameProducer&) = delete;

        bool Attach() noexcept;
        void Detach() noexcept;
        bool IsAttached() const noexcept;
        bool TryWrite(
            const std::uint8_t* source,
            std::uint32_t sourceRowPitch,
            std::uint32_t width,
            std::uint32_t height,
            std::uint64_t sourceTimestampNanos) noexcept;

    private:
        bool Validate() const noexcept;

        HANDLE mapping_ = nullptr;
        std::uint8_t* view_ = nullptr;
        FrameHeader* header_ = nullptr;
        wchar_t mappingName_[MappingNameV2Chars] = {};
    };
}
