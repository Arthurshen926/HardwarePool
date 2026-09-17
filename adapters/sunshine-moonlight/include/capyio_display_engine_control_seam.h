#ifndef CAPYIO_DISPLAY_ENGINE_CONTROL_SEAM_H
#define CAPYIO_DISPLAY_ENGINE_CONTROL_SEAM_H

#include <mutex>
#include <cstdint>
#include <string>

namespace config {
  std::uint64_t set_runtime_output_name_override_with_lease(std::string output_name);
  bool clear_runtime_output_name_override_if_lease(std::uint64_t lease);
  std::uint64_t acquire_capyio_display_session_config(
    std::string output_name,
    std::uint16_t sunshine_port_base
  );
  bool release_capyio_display_session_config(std::uint64_t lease);
}  // namespace config

namespace nvhttp {
  std::mutex &stream_lifecycle_mutex();
  void mark_client_last_seen(const std::string &uuid);
}  // namespace nvhttp

#endif
