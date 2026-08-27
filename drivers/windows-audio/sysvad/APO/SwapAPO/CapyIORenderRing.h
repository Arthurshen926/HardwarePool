#pragma once

#include <windows.h>
#include <cstddef>
#include <cstdint>

// This layout is a versioned cross-process ABI. Keep it in sync with the
// Rust broker consumer and drivers/windows-audio/IPC_CONTRACT.md.
namespace capyio::render_ring
{
constexpr std::uint32_t kMagic = 0x524F4950; // "PIOR" in little endian.
constexpr std::uint16_t kVersion = 1;
constexpr std::uint16_t kHeaderSize = 128;
constexpr std::uint16_t kSampleFormatFloat32Le = 1;
constexpr std::uint32_t kMaxSlots = 64;
constexpr std::uint32_t kMaxPayloadBytes = 16 * 1024;
constexpr wchar_t kMappingName[] = L"Global\\CapyIO.RenderRing.v1";

struct alignas(64) Header
{
    std::uint32_t magic;
    std::uint16_t version;
    std::uint16_t header_size;
    std::uint32_t total_size;
    std::uint32_t slot_count;
    std::uint32_t slot_stride;
    std::uint32_t payload_capacity;
    std::uint32_t sample_rate;
    std::uint16_t channels;
    std::uint16_t sample_format;
    std::uint64_t generation;
    volatile LONG64 write_sequence;
    volatile LONG64 read_sequence;
    volatile LONG64 dropped_blocks;
    volatile LONG64 produced_blocks;
    volatile LONG64 attach_attempts;
    volatile LONG64 attach_successes;
    volatile LONG last_sample_rate;
    volatile LONG last_channels;
    volatile LONG last_stage;
    volatile LONG last_error;
    std::uint8_t reserved[24];
};

static_assert(sizeof(Header) == kHeaderSize);
static_assert(offsetof(Header, write_sequence) % alignof(LONG64) == 0);

struct SlotHeader
{
    std::uint64_t generation;
    std::uint32_t byte_count;
    std::uint32_t frame_count;
};

static_assert(sizeof(SlotHeader) == 16);

class Producer final
{
public:
    Producer() noexcept = default;
    ~Producer() noexcept;

    Producer(const Producer&) = delete;
    Producer& operator=(const Producer&) = delete;

    bool Attach(std::uint32_t sample_rate, std::uint32_t channels) noexcept;
    void Detach() noexcept;
    void TryWrite(const float* samples, std::uint32_t frame_count, std::uint32_t channels) noexcept;

private:
    bool Validate(std::uint32_t sample_rate, std::uint32_t channels) const noexcept;
    void CountDrop() noexcept;

    HANDLE mapping_ = nullptr;
    std::uint8_t* view_ = nullptr;
    Header* header_ = nullptr;
};
} // namespace capyio::render_ring
