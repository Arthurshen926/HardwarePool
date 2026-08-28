#pragma once

#include <windows.h>
#include <cstddef>
#include <cstdint>

// Versioned SPSC frame FIFO shared by the microphone ingress and capture APOs.
// Keep this layout in sync with capyio-service/src/capture_ring.rs and
// drivers/windows-audio/IPC_CONTRACT.md.
namespace capyio::capture_ring
{
constexpr std::uint32_t kMagic = 0x434F4950; // "PIOC" in little endian.
constexpr std::uint16_t kVersion = 1;
constexpr std::uint16_t kHeaderSize = 128;
constexpr std::uint16_t kSampleFormatFloat32Le = 1;
constexpr std::uint32_t kSampleRate = 48'000;
constexpr std::uint16_t kChannels = 1;
constexpr std::uint32_t kFrameCapacity = 16'384;
constexpr std::uint32_t kBytesPerFrame = sizeof(float);
constexpr std::uint32_t kTotalSize = kHeaderSize + kFrameCapacity * kBytesPerFrame;
constexpr wchar_t kMappingName[] = L"Global\\CapyIO.CaptureRing.v1";

struct alignas(64) Header
{
    std::uint32_t magic;
    std::uint16_t version;
    std::uint16_t header_size;
    std::uint32_t total_size;
    std::uint32_t frame_capacity;
    std::uint32_t bytes_per_frame;
    std::uint32_t sample_rate;
    std::uint16_t channels;
    std::uint16_t sample_format;
    std::uint32_t reserved0;
    std::uint64_t generation;
    volatile LONG64 write_frame_sequence;
    volatile LONG64 read_frame_sequence;
    volatile LONG64 dropped_frames;
    volatile LONG64 produced_frames;
    volatile LONG64 consumed_frames;
    volatile LONG64 underrun_frames;
    volatile LONG64 producer_attach_attempts;
    volatile LONG64 producer_attach_successes;
    volatile LONG64 consumer_attach_attempts;
    volatile LONG64 consumer_attach_successes;
    volatile LONG last_stage;
    volatile LONG last_error;
};

static_assert(sizeof(Header) == kHeaderSize);
static_assert(offsetof(Header, write_frame_sequence) % alignof(LONG64) == 0);

class Mapping
{
public:
    Mapping() noexcept = default;
    virtual ~Mapping() noexcept;

    Mapping(const Mapping&) = delete;
    Mapping& operator=(const Mapping&) = delete;

    void Detach() noexcept;

protected:
    bool AttachCommon(bool producer) noexcept;
    bool Validate() const noexcept;
    float* Frames() const noexcept;

    HANDLE mapping_ = nullptr;
    std::uint8_t* view_ = nullptr;
    Header* header_ = nullptr;
};

class Producer final : public Mapping
{
public:
    bool Attach(std::uint32_t sample_rate, std::uint32_t input_channels) noexcept;

    // Writes the complete callback or drops it. Mono is copied; stereo is
    // downmixed with equal weights. No partial callback is committed.
    void TryWrite(
        const float* input,
        std::uint32_t frame_count,
        std::uint32_t input_channels) noexcept;
};

class Consumer final : public Mapping
{
public:
    bool Attach(std::uint32_t sample_rate, std::uint32_t output_channels) noexcept;

    // Copies available mono frames and zero-fills the remainder. Returns the
    // number of non-silent frames copied from the ring.
    std::uint32_t TryRead(float* output, std::uint32_t frame_count) noexcept;
};
} // namespace capyio::capture_ring
