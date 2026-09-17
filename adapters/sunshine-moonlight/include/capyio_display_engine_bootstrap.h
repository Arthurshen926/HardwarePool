#ifndef CAPYIO_DISPLAY_ENGINE_BOOTSTRAP_H
#define CAPYIO_DISPLAY_ENGINE_BOOTSTRAP_H

#include "capyio_display_engine_v1.h"

#include <array>
#include <cstddef>
#include <cstdint>

namespace capyio::display_engine {

enum class BootstrapState : std::uint32_t {
  idle = 0,
  running = 1,
  cleanup_required = 2,
};

enum class ListenerStartResult : std::uint32_t {
  ready = 0,
  bind_failed = 1,
  timeout_quiescent = 2,
  timeout_owned = 3,
};

enum class BootstrapResult : std::int32_t {
  ok = 0,
  invalid_argument = -1,
  invalid_state = -2,
  mail_start_failed = -3,
  platform_start_failed = -4,
  encoder_probe_failed = -5,
  listener_bind_failed = -6,
  timeout = -7,
};

constexpr std::size_t launch_session_key_bytes = 16;
constexpr std::size_t launch_ping_payload_bytes = 8;

// Supplied only after the receiver has authenticated and the Route has been
// authorized. This is an internal native contract, not a network or public ABI
// record. Callers must not log or persist it.
struct LaunchSessionMaterial {
  std::uint32_t launch_session_id;
  std::array<std::uint8_t, launch_session_key_bytes> key;
  std::uint32_t key_id;
  std::array<std::uint8_t, launch_ping_payload_bytes> ping_payload;
  std::uint32_t control_connect_data;
};

struct AuthorizedStreamSelection {
  std::uint32_t width;
  std::uint32_t height;
  std::uint32_t refresh_millihertz;
  std::uint32_t codec;
  std::uint32_t dynamic_range;
  std::uint32_t bitrate_kbps;
};

struct AnnouncedStreamSelection {
  std::int64_t width;
  std::int64_t height;
  std::int64_t refresh_millihertz;
  std::int64_t codec;
  std::int64_t dynamic_range;
  std::int64_t bitrate_kbps;
};

[[nodiscard]] bool authorized_stream_selection_matches(
    const AuthorizedStreamSelection &authorized,
    const AnnouncedStreamSelection &announced) noexcept;

// Pointer-free, fully validated input to the upstream-specific conversion.
// The flags are fixed policy assertions: CapyIO owns the selected monitor and
// this Adapter must not enable audio, input or an upstream virtual display.
struct LaunchSessionPlan {
  std::uint32_t launch_session_id;
  std::array<std::uint8_t, launch_session_key_bytes> key;
  std::array<std::uint8_t, launch_session_key_bytes> iv;
  std::array<std::uint8_t, launch_ping_payload_bytes> ping_payload;
  std::uint32_t control_connect_data;
  std::array<std::uint8_t, 16> display_sink_identity;
  std::array<std::uint8_t, CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY>
      capture_output_utf8;
  AuthorizedStreamSelection selection;
  bool encrypted_rtsp;
  bool audio_enabled;
  bool input_enabled;
  bool upstream_virtual_display_enabled;
  bool display_config_preapplied;
};

enum class LaunchSessionResult : std::int32_t {
  ok = 0,
  invalid_start = -1,
  invalid_material = -2,
  invalid_state = -3,
  allocation_failed = -4,
};

[[nodiscard]] LaunchSessionResult build_launch_session_plan(
    const CapyIoDisplayEngineStartV2 &start,
    const LaunchSessionMaterial &material,
    LaunchSessionPlan &output) noexcept;

// The listener start hook must return bind_failed or timeout_quiescent only
// after its worker and readiness callback are quiescent. timeout_owned means a
// worker remains owned and must be stopped before base resources are released.
// A ready listener is owned until stop_listener returns true.
struct BootstrapHooks {
  void *context;
  bool (*start_mail)(void *context) noexcept;
  void (*stop_mail)(void *context) noexcept;
  bool (*start_platform)(void *context) noexcept;
  void (*stop_platform)(void *context) noexcept;
  bool (*probe_encoder)(void *context) noexcept;
  ListenerStartResult (*start_listener)(
      void *context, std::uint32_t timeout_millis) noexcept;
  bool (*stop_listener)(
      void *context, std::uint32_t timeout_millis) noexcept;
};

#ifdef CAPYIO_VIDEO_ONLY
struct UpstreamRuntime;

[[nodiscard]] UpstreamRuntime *create_upstream_runtime() noexcept;
[[nodiscard]] bool destroy_upstream_runtime(UpstreamRuntime *runtime) noexcept;
[[nodiscard]] BootstrapHooks upstream_bootstrap_hooks(
    UpstreamRuntime *runtime) noexcept;
[[nodiscard]] LaunchSessionResult stage_upstream_launch_session(
    UpstreamRuntime *runtime,
    const CapyIoDisplayEngineStartV2 &start,
    const LaunchSessionMaterial &material) noexcept;
[[nodiscard]] bool has_staged_upstream_launch_session(
    const UpstreamRuntime *runtime) noexcept;
void discard_staged_upstream_launch_session(UpstreamRuntime *runtime) noexcept;
#endif

class Bootstrap final {
 public:
  explicit Bootstrap(BootstrapHooks hooks) noexcept;

  Bootstrap(const Bootstrap &) = delete;
  Bootstrap &operator=(const Bootstrap &) = delete;

  [[nodiscard]] BootstrapState state() const noexcept;
  [[nodiscard]] BootstrapResult start(std::uint32_t timeout_millis) noexcept;
  [[nodiscard]] BootstrapResult stop(std::uint32_t timeout_millis) noexcept;

 private:
  [[nodiscard]] bool hooks_valid() const noexcept;
  void release_base_resources() noexcept;

  BootstrapHooks hooks_;
  BootstrapState state_ {BootstrapState::idle};
  bool mail_owned_ {false};
  bool platform_owned_ {false};
};

}  // namespace capyio::display_engine

#endif
