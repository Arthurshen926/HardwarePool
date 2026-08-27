#include "CapyIORenderRing.h"

#include <limits>

namespace capyio::render_ring
{
Producer::~Producer() noexcept
{
    Detach();
}

bool Producer::Attach(std::uint32_t sample_rate, std::uint32_t channels) noexcept
{
    Detach();

    mapping_ = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, kMappingName);
    if (mapping_ == nullptr)
    {
        return false;
    }

    view_ = static_cast<std::uint8_t*>(MapViewOfFile(mapping_, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0));
    if (view_ == nullptr)
    {
        CloseHandle(mapping_);
        mapping_ = nullptr;
        return false;
    }

    header_ = reinterpret_cast<Header*>(view_);
    InterlockedIncrement64(&header_->attach_attempts);
    InterlockedExchange(&header_->last_sample_rate, static_cast<LONG>(sample_rate));
    InterlockedExchange(&header_->last_channels, static_cast<LONG>(channels));
    InterlockedExchange(&header_->last_stage, 1);
    InterlockedExchange(&header_->last_error, ERROR_SUCCESS);
    if (!Validate(sample_rate, channels))
    {
        InterlockedExchange(&header_->last_stage, 2);
        InterlockedExchange(&header_->last_error, ERROR_INVALID_DATA);
        Detach();
        return false;
    }

    InterlockedIncrement64(&header_->attach_successes);
    InterlockedExchange(&header_->last_stage, 3);
    return true;
}

void Producer::Detach() noexcept
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

bool Producer::Validate(std::uint32_t sample_rate, std::uint32_t channels) const noexcept
{
    if (header_ == nullptr || header_->magic != kMagic || header_->version != kVersion ||
        header_->header_size != kHeaderSize || header_->sample_rate != sample_rate ||
        header_->channels != channels ||
        header_->sample_format != kSampleFormatFloat32Le || header_->slot_count < 2 ||
        header_->slot_count > kMaxSlots || header_->payload_capacity == 0 ||
        header_->payload_capacity > kMaxPayloadBytes || header_->slot_stride < sizeof(SlotHeader) ||
        header_->slot_stride - sizeof(SlotHeader) < header_->payload_capacity)
    {
        return false;
    }

    const std::uint64_t slots_bytes =
        static_cast<std::uint64_t>(header_->slot_count) * header_->slot_stride;
    const std::uint64_t required = static_cast<std::uint64_t>(header_->header_size) + slots_bytes;
    return required <= header_->total_size && required <= (std::numeric_limits<std::uint32_t>::max)();
}

void Producer::CountDrop() noexcept
{
    if (header_ != nullptr)
    {
        InterlockedIncrement64(&header_->dropped_blocks);
    }
}

void Producer::TryWrite(
    const float* samples,
    std::uint32_t frame_count,
    std::uint32_t channels) noexcept
{
    if (header_ == nullptr || samples == nullptr || channels != header_->channels || frame_count == 0)
    {
        return;
    }

    const std::uint64_t byte_count64 =
        static_cast<std::uint64_t>(frame_count) * channels * sizeof(float);
    if (byte_count64 > header_->payload_capacity || byte_count64 > (std::numeric_limits<std::uint32_t>::max)())
    {
        CountDrop();
        return;
    }

    const LONG64 write = InterlockedCompareExchange64(&header_->write_sequence, 0, 0);
    const LONG64 read = InterlockedCompareExchange64(&header_->read_sequence, 0, 0);
    if (write < 0 || read < 0 || write < read ||
        static_cast<std::uint64_t>(write - read) >= header_->slot_count)
    {
        CountDrop();
        return;
    }

    const std::uint64_t slot_index = static_cast<std::uint64_t>(write) % header_->slot_count;
    std::uint8_t* slot_base = view_ + header_->header_size + slot_index * header_->slot_stride;
    auto* slot = reinterpret_cast<SlotHeader*>(slot_base);
    slot->generation = header_->generation;
    slot->byte_count = static_cast<std::uint32_t>(byte_count64);
    slot->frame_count = frame_count;
    CopyMemory(slot_base + sizeof(SlotHeader), samples, static_cast<SIZE_T>(byte_count64));

    MemoryBarrier();
    InterlockedExchange64(&header_->write_sequence, write + 1);
    InterlockedIncrement64(&header_->produced_blocks);
}
} // namespace capyio::render_ring
