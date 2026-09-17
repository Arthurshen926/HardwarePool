// CapyIO-owned, display-only configuration seam for the private engine target.
//
// The bridge still advertises no usable engine features. These zero/default
// values exist only to compile the retained media lifecycle without importing
// Vibepollo's web UI, file parser, hot reload, session history, or global
// display-recovery workers. A later slice must populate the exact session
// values before the feature probe can be enabled.

#include "src/config.h"
#include "capyio_display_engine_control_seam.h"

#include <chrono>
#include <mutex>
#include <optional>
#include <shared_mutex>
#include <string>
#include <string_view>
#include <unordered_map>
#include <utility>

namespace {
  std::shared_mutex apply_gate;
  std::mutex runtime_gate;
  std::optional<std::string> output_name_override;
  std::unordered_map<std::string, std::string> runtime_overrides;
  std::uint64_t output_override_lease = 0;
  std::uint64_t active_capyio_session_lease = 0;
  std::optional<std::string> saved_session_output;
  std::uint16_t saved_session_port_base = 0;
  std::string saved_session_address_family;
  std::string saved_session_bind_address;
  std::chrono::milliseconds saved_session_ping_timeout {};
}

namespace config {
  video_t video {};
  audio_t audio {};
  stream_t stream {};
  nvhttp_t nvhttp {};
  input_t input {};
  frame_limiter_t frame_limiter {};
  rtss_t rtss {};
  lossless_scaling_t lossless_scaling {};
  sunshine_t sunshine {};

  void apply_config_now() {}

  void mark_deferred_reload() {}

  void maybe_apply_deferred() {}

  std::shared_lock<std::shared_mutex> acquire_apply_read_gate() {
    return std::shared_lock {apply_gate};
  }

  void record_active_adapter_config() {}

  void set_runtime_config_overrides(std::unordered_map<std::string, std::string> overrides) {
    std::lock_guard lock {runtime_gate};
    runtime_overrides = std::move(overrides);
  }

  void clear_runtime_config_overrides() {
    std::lock_guard lock {runtime_gate};
    runtime_overrides.clear();
  }

  std::unordered_map<std::string, std::string> runtime_config_overrides_snapshot() {
    std::lock_guard lock {runtime_gate};
    return runtime_overrides;
  }

  bool has_runtime_config_override(std::string_view key) {
    std::lock_guard lock {runtime_gate};
    return runtime_overrides.contains(std::string {key});
  }

  bool runtime_config_override_enabled(std::string_view key) {
    std::lock_guard lock {runtime_gate};
    const auto value = runtime_overrides.find(std::string {key});
    return value != runtime_overrides.end() &&
           (value->second == "true" || value->second == "1" || value->second == "enabled");
  }

  bool has_runtime_config_overrides() {
    std::lock_guard lock {runtime_gate};
    return !runtime_overrides.empty();
  }

  void set_runtime_output_name_override(std::optional<std::string> output_name) {
    std::lock_guard lock {runtime_gate};
    if (active_capyio_session_lease != 0) {
      return;
    }
    output_name_override = std::move(output_name);
    ++output_override_lease;
  }

  std::optional<std::string> runtime_output_name_override() {
    std::lock_guard lock {runtime_gate};
    return output_name_override;
  }

  std::string get_active_output_name() {
    std::lock_guard lock {runtime_gate};
    return output_name_override.value_or(video.output_name);
  }

#ifdef _WIN32
  runtime_output_override_lease_t set_runtime_output_name_override_with_lease(std::string output_name) {
    std::lock_guard lock {runtime_gate};
    if (active_capyio_session_lease != 0) {
      return 0;
    }
    output_name_override = std::move(output_name);
    return ++output_override_lease;
  }

  bool clear_runtime_output_name_override_if_lease(runtime_output_override_lease_t lease) {
    std::lock_guard lock {runtime_gate};
    if (active_capyio_session_lease != 0 || lease == 0 || lease != output_override_lease) {
      return false;
    }
    output_name_override.reset();
    ++output_override_lease;
    return true;
  }

  std::uint64_t acquire_capyio_display_session_config(
    std::string output_name,
    const std::uint16_t sunshine_port_base
  ) {
    std::lock_guard lock {runtime_gate};
    if (output_name.empty() || sunshine_port_base < 1024 ||
        active_capyio_session_lease != 0) {
      return 0;
    }
    saved_session_output = output_name_override;
    saved_session_port_base = sunshine.port;
    saved_session_address_family = sunshine.address_family;
    saved_session_bind_address = sunshine.bind_address;
    saved_session_ping_timeout = stream.ping_timeout;
    output_name_override = std::move(output_name);
    sunshine.port = sunshine_port_base;
    sunshine.address_family = "ipv4";
    sunshine.bind_address.clear();
    // The full upstream configuration defaults this RTSP launch reservation to
    // ten seconds. The display-only target owns a zero-initialized config seam,
    // so set the same bounded window explicitly for the active CapyIO lease.
    stream.ping_timeout = std::chrono::seconds {10};
    active_capyio_session_lease = ++output_override_lease;
    return active_capyio_session_lease;
  }

  bool release_capyio_display_session_config(const std::uint64_t lease) {
    std::lock_guard lock {runtime_gate};
    if (lease == 0 || lease != active_capyio_session_lease) {
      return false;
    }
    output_name_override = std::move(saved_session_output);
    saved_session_output.reset();
    sunshine.port = saved_session_port_base;
    saved_session_port_base = 0;
    sunshine.address_family = std::move(saved_session_address_family);
    sunshine.bind_address = std::move(saved_session_bind_address);
    saved_session_address_family.clear();
    saved_session_bind_address.clear();
    stream.ping_timeout = saved_session_ping_timeout;
    saved_session_ping_timeout = {};
    active_capyio_session_lease = 0;
    ++output_override_lease;
    return true;
  }

  void request_deferred_virtual_output_reapply_shutdown() {}

  void join_deferred_virtual_output_reapply_worker() {}
#endif
}  // namespace config

namespace nvhttp {
  std::mutex &stream_lifecycle_mutex() {
    static std::mutex lifecycle_gate;
    return lifecycle_gate;
  }

  void mark_client_last_seen(const std::string &) {}
}  // namespace nvhttp
