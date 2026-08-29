#include "CapyIOCaptureRing.h"

namespace capyio::capture_ring
{
void RecordDiagnostic(LONG stage, LONG error) noexcept
{
    HANDLE mapping = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, kMappingName);
    if (mapping == nullptr)
    {
        return;
    }
    auto* view = static_cast<std::uint8_t*>(
        MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, kHeaderSize));
    if (view != nullptr)
    {
        auto* header = reinterpret_cast<Header*>(view);
        InterlockedExchange(&header->last_stage, stage);
        InterlockedExchange(&header->last_error, error);
        UnmapViewOfFile(view);
    }
    CloseHandle(mapping);
}

Mapping::~Mapping() noexcept
{
    Detach();
}

bool Mapping::AttachCommon(bool producer) noexcept
{
    Detach();
    mapping_ = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, kMappingName);
    if (mapping_ == nullptr)
    {
        return false;
    }

    view_ = static_cast<std::uint8_t*>(
        MapViewOfFile(mapping_, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, kTotalSize));
    if (view_ == nullptr)
    {
        CloseHandle(mapping_);
        mapping_ = nullptr;
        return false;
    }

    header_ = reinterpret_cast<Header*>(view_);
    volatile LONG64* attempts = producer ? &header_->producer_attach_attempts
                                         : &header_->consumer_attach_attempts;
    InterlockedIncrement64(attempts);
    InterlockedExchange(&header_->last_stage, producer ? 101 : 201);
    InterlockedExchange(&header_->last_error, ERROR_SUCCESS);
    if (!Validate())
    {
        InterlockedExchange(&header_->last_stage, producer ? 102 : 202);
        InterlockedExchange(&header_->last_error, ERROR_INVALID_DATA);
        Detach();
        return false;
    }

    volatile LONG64* successes = producer ? &header_->producer_attach_successes
                                          : &header_->consumer_attach_successes;
    InterlockedIncrement64(successes);
    InterlockedExchange(&header_->last_stage, producer ? 103 : 203);
    return true;
}

void Mapping::Detach() noexcept
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

bool Mapping::Validate() const noexcept
{
    return header_ != nullptr && header_->magic == kMagic && header_->version == kVersion &&
           header_->header_size == kHeaderSize && header_->total_size == kTotalSize &&
           header_->frame_capacity == kFrameCapacity &&
           header_->bytes_per_frame == kBytesPerFrame && header_->sample_rate == kSampleRate &&
           header_->channels == kChannels &&
           header_->sample_format == kSampleFormatFloat32Le;
}

float* Mapping::Frames() const noexcept
{
    return reinterpret_cast<float*>(view_ + kHeaderSize);
}

bool Producer::Attach(std::uint32_t sample_rate, std::uint32_t input_channels) noexcept
{
    if (sample_rate != kSampleRate || (input_channels != 1 && input_channels != 2))
    {
        return false;
    }
    return AttachCommon(true);
}

void Producer::TryWrite(
    const float* input,
    std::uint32_t frame_count,
    std::uint32_t input_channels) noexcept
{
    if (header_ == nullptr || input == nullptr || frame_count == 0 ||
        frame_count > kFrameCapacity || (input_channels != 1 && input_channels != 2))
    {
        return;
    }

    const LONG64 write = InterlockedCompareExchange64(&header_->write_frame_sequence, 0, 0);
    const LONG64 read = InterlockedCompareExchange64(&header_->read_frame_sequence, 0, 0);
    if (write < 0 || read < 0 || write < read ||
        static_cast<std::uint64_t>(write - read) > kFrameCapacity)
    {
        InterlockedExchangeAdd64(&header_->dropped_frames, frame_count);
        return;
    }

    const std::uint64_t used = static_cast<std::uint64_t>(write - read);
    if (frame_count > kFrameCapacity - used)
    {
        InterlockedExchangeAdd64(&header_->dropped_frames, frame_count);
        return;
    }

    float* frames = Frames();
    for (std::uint32_t index = 0; index < frame_count; ++index)
    {
        const std::uint64_t destination =
            (static_cast<std::uint64_t>(write) + index) % kFrameCapacity;
        if (input_channels == 1)
        {
            frames[destination] = input[index];
        }
        else
        {
            const std::uint32_t source = index * 2;
            frames[destination] = (input[source] + input[source + 1]) * 0.5F;
        }
    }

    MemoryBarrier();
    InterlockedExchange64(&header_->write_frame_sequence, write + frame_count);
    InterlockedExchangeAdd64(&header_->produced_frames, frame_count);
}

bool Consumer::Attach(std::uint32_t sample_rate, std::uint32_t output_channels) noexcept
{
    if (sample_rate != kSampleRate || output_channels != kChannels)
    {
        return false;
    }
    if (!AttachCommon(false))
    {
        return false;
    }

    // A microphone capture session is a live view, not a recording backlog.
    // The producer can keep the bounded ring full while no capture client is
    // active. Discard those pre-attach frames so a later application never
    // replays stale speech after the phone has already disconnected.
    const LONG64 write = InterlockedCompareExchange64(&header_->write_frame_sequence, 0, 0);
    if (write < 0)
    {
        Detach();
        return false;
    }
    InterlockedExchange64(&header_->read_frame_sequence, write);
    return true;
}

std::uint32_t Consumer::TryRead(float* output, std::uint32_t frame_count) noexcept
{
    if (output == nullptr || frame_count == 0 || frame_count > kFrameCapacity)
    {
        return 0;
    }
    ZeroMemory(output, static_cast<SIZE_T>(frame_count) * sizeof(float));
    if (header_ == nullptr)
    {
        return 0;
    }

    const LONG64 read = InterlockedCompareExchange64(&header_->read_frame_sequence, 0, 0);
    const LONG64 write = InterlockedCompareExchange64(&header_->write_frame_sequence, 0, 0);
    if (read < 0 || write < read || static_cast<std::uint64_t>(write - read) > kFrameCapacity)
    {
        InterlockedExchangeAdd64(&header_->underrun_frames, frame_count);
        return 0;
    }

    const std::uint32_t available = static_cast<std::uint32_t>(write - read);
    const std::uint32_t copied = available < frame_count ? available : frame_count;
    float* frames = Frames();
    for (std::uint32_t index = 0; index < copied; ++index)
    {
        const std::uint64_t source =
            (static_cast<std::uint64_t>(read) + index) % kFrameCapacity;
        output[index] = frames[source];
    }

    MemoryBarrier();
    InterlockedExchange64(&header_->read_frame_sequence, read + copied);
    InterlockedExchangeAdd64(&header_->consumed_frames, copied);
    if (copied < frame_count)
    {
        InterlockedExchangeAdd64(&header_->underrun_frames, frame_count - copied);
    }
    return copied;
}
} // namespace capyio::capture_ring
