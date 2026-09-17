#include "CapyIoDisplayProducer.h"

#include <limits>

namespace CapyIoDisplayHandoff
{
FrameProducer::FrameProducer(
    const std::array<std::uint8_t, MonitorIdentityBytes>& monitorIdentity) noexcept
{
    constexpr wchar_t Hex[] = L"0123456789abcdef";
    constexpr std::size_t PrefixChars =
        (sizeof(MappingNameV2Prefix) / sizeof(wchar_t)) - 1;
    static_assert(PrefixChars + MonitorIdentityBytes * 2 + 1 == MappingNameV2Chars);
    CopyMemory(mappingName_, MappingNameV2Prefix, PrefixChars * sizeof(wchar_t));
    for (std::size_t index = 0; index < monitorIdentity.size(); ++index)
    {
        mappingName_[PrefixChars + index * 2] = Hex[monitorIdentity[index] >> 4];
        mappingName_[PrefixChars + index * 2 + 1] = Hex[monitorIdentity[index] & 0x0f];
    }
    mappingName_[MappingNameV2Chars - 1] = L'\0';
}

FrameProducer::~FrameProducer() noexcept
{
    Detach();
}

bool FrameProducer::Attach() noexcept
{
    Detach();
    mapping_ = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, mappingName_);
    if (mapping_ == nullptr)
    {
        return false;
    }

    view_ = static_cast<std::uint8_t*>(
        MapViewOfFile(mapping_, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, TotalMappingBytes));
    if (view_ == nullptr)
    {
        CloseHandle(mapping_);
        mapping_ = nullptr;
        return false;
    }
    header_ = reinterpret_cast<FrameHeader*>(view_);
    if (!Validate())
    {
        Detach();
        return false;
    }
    return true;
}

void FrameProducer::Detach() noexcept
{
    header_ = nullptr;
    if (view_ != nullptr)
    {
        UnmapViewOfFile(view_);
        view_ = nullptr;
    }
    if (mapping_ != nullptr)
    {
        CloseHandle(mapping_);
        mapping_ = nullptr;
    }
}

bool FrameProducer::IsAttached() const noexcept
{
    return header_ != nullptr;
}

bool FrameProducer::Validate() const noexcept
{
    if (header_ == nullptr ||
        header_->MagicValue != Magic ||
        header_->Version != AbiVersion ||
        header_->HeaderSize != HeaderBytes)
    {
        return false;
    }
    const LONG64 generation = InterlockedCompareExchange64(
        reinterpret_cast<volatile LONG64*>(
            const_cast<std::uint64_t*>(&header_->Generation)), 0, 0);
    const LONG64 sequence = InterlockedCompareExchange64(
        reinterpret_cast<volatile LONG64*>(
            const_cast<std::uint64_t*>(&header_->SequenceLock)), 0, 0);
    return generation > 0 && sequence >= 0 && (sequence & 1) == 0;
}

bool FrameProducer::TryWrite(
    const std::uint8_t* source,
    std::uint32_t sourceRowPitch,
    std::uint32_t width,
    std::uint32_t height,
    std::uint64_t sourceTimestampNanos) noexcept
{
    if (!Validate())
    {
        Detach();
        return false;
    }
    if (source == nullptr || width == 0 || height == 0 ||
        width > MaxWidth || height > MaxHeight)
    {
        return false;
    }

    const std::uint64_t packedStride64 = static_cast<std::uint64_t>(width) * 4;
    const std::uint64_t payloadBytes = packedStride64 * height;
    if (packedStride64 > (std::numeric_limits<std::uint32_t>::max)() ||
        sourceRowPitch < packedStride64 || payloadBytes > MaxPayloadBytes)
    {
        return false;
    }

    auto* sequenceAddress =
        reinterpret_cast<volatile LONG64*>(&header_->SequenceLock);
    const LONG64 sequence = InterlockedCompareExchange64(sequenceAddress, 0, 0);
    if (sequence < 0 || (sequence & 1) != 0 ||
        sequence > (std::numeric_limits<LONG64>::max)() - 2)
    {
        return false;
    }

    InterlockedExchange64(sequenceAddress, sequence + 1);
    header_->Width = width;
    header_->Height = height;
    header_->StrideBytes = static_cast<std::uint32_t>(packedStride64);
    header_->PixelFormat = PixelFormatBgra8;
    header_->PayloadBytes = payloadBytes;
    header_->SourceTimestampNanos = sourceTimestampNanos;
    header_->Flags = 0;
    header_->Reserved = 0;

    std::uint8_t* destination = view_ + HeaderBytes;
    const auto packedStride = static_cast<SIZE_T>(packedStride64);
    for (std::uint32_t row = 0; row < height; ++row)
    {
        CopyMemory(
            destination + static_cast<SIZE_T>(row) * packedStride,
            source + static_cast<SIZE_T>(row) * sourceRowPitch,
            packedStride);
    }
    MemoryBarrier();
    InterlockedExchange64(sequenceAddress, sequence + 2);
    return true;
}
}
