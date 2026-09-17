#ifndef CAPYIO_DISPLAY_ENGINE_EXPORTS
#define CAPYIO_DISPLAY_ENGINE_EXPORTS
#endif
#include "capyio_display_engine_v1.h"

#include <cstdint>
#include <cstring>
#include <new>

#ifdef CAPYIO_VIDEO_ONLY
#include "capyio_display_engine_bootstrap.h"
#include "capyio_display_engine_control_seam.h"

namespace capyio::display_engine {
[[nodiscard]] bool request_upstream_key_frame(
    UpstreamRuntime *runtime) noexcept;
}
#endif

namespace {

struct EngineHandle {
  CapyIoDisplayEngineCallbacksV2 callbacks;
  CapyIoDisplayEngineStartV2 start_config;
#ifdef CAPYIO_VIDEO_ONLY
  std::uint64_t output_override_lease;
  capyio::display_engine::UpstreamRuntime *upstream_runtime;
  capyio::display_engine::Bootstrap *bootstrap;
#endif
};

#ifdef CAPYIO_VIDEO_ONLY
constexpr std::uint64_t enabled_video_features =
    CAPYIO_ENGINE_FEATURE_CAPTURE |
    CAPYIO_ENGINE_FEATURE_HARDWARE_ENCODE |
    CAPYIO_ENGINE_FEATURE_PACING |
    CAPYIO_ENGINE_FEATURE_PACKET_RECOVERY |
    CAPYIO_ENGINE_FEATURE_VIDEO_CONTROL |
    CAPYIO_ENGINE_FEATURE_HDR10 |
    CAPYIO_ENGINE_FEATURE_HEVC;
// The service-owned worker lifecycle and the Android display-only GameStream
// receiver now both retain, interrupt and wipe one authenticated session.
// Probe may therefore advertise the compiled video feature set.
constexpr bool lifecycle_ready = true;

int32_t bootstrap_status(
    const capyio::display_engine::BootstrapResult result) noexcept {
  using capyio::display_engine::BootstrapResult;
  switch (result) {
    case BootstrapResult::ok:
      return CAPYIO_ENGINE_OK;
    case BootstrapResult::invalid_argument:
      return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    case BootstrapResult::invalid_state:
      return CAPYIO_ENGINE_E_INVALID_STATE;
    case BootstrapResult::timeout:
      return CAPYIO_ENGINE_E_TIMEOUT;
    case BootstrapResult::mail_start_failed:
    case BootstrapResult::platform_start_failed:
    case BootstrapResult::encoder_probe_failed:
    case BootstrapResult::listener_bind_failed:
      return CAPYIO_ENGINE_E_INTERNAL;
  }
  return CAPYIO_ENGINE_E_INTERNAL;
}
#endif

void clear_start_config(EngineHandle &engine) noexcept {
#ifdef CAPYIO_VIDEO_ONLY
  if (engine.output_override_lease != 0) {
    config::release_capyio_display_session_config(
        engine.output_override_lease);
    engine.output_override_lease = 0;
  }
#endif
  std::memset(&engine.start_config, 0, sizeof(engine.start_config));
}

bool nonzero_identity(const uint8_t value[16]) noexcept {
  for (unsigned index = 0; index < 16; ++index) {
    if (value[index] != 0) {
      return true;
    }
  }
  return false;
}

bool valid_capture_output(
    const uint8_t value[CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY]) noexcept {
  constexpr char prefix[] = "\\\\.\\DISPLAY";
  constexpr unsigned prefix_length = sizeof(prefix) - 1;
  for (unsigned index = 0; index < prefix_length; ++index) {
    if (value[index] != static_cast<uint8_t>(prefix[index])) {
      return false;
    }
  }
  unsigned end = prefix_length;
  while (end < CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY && value[end] != 0) {
    if (value[end] < '0' || value[end] > '9' ||
        (end == prefix_length && value[end] == '0')) {
      return false;
    }
    ++end;
  }
  if (end == prefix_length || end == CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY) {
    return false;
  }
  for (unsigned index = end + 1;
       index < CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY; ++index) {
    if (value[index] != 0) {
      return false;
    }
  }
  return true;
}

bool valid_timeout(uint32_t timeout_millis) noexcept {
  return timeout_millis >= 100 && timeout_millis <= 10000;
}

bool valid_port_namespace(const CapyIoDisplayEngineStartV2 &start) noexcept {
  if (start.rtsp_port < 21) {
    return false;
  }
  const auto base = static_cast<uint32_t>(start.rtsp_port) - 21;
  return base >= 1024 && base != 47989 &&
         start.https_port == base - 5 && start.video_port == base + 9 &&
         start.control_port == base + 10;
}

bool valid_start_config(const CapyIoDisplayEngineStartV2 *start) noexcept {
  return start != nullptr && start->struct_size == sizeof(*start) &&
         start->abi_version == CAPYIO_DISPLAY_ENGINE_ABI_V2 &&
         nonzero_identity(start->adapter_instance_id) &&
         nonzero_identity(start->display_capability_id) &&
         valid_capture_output(start->capture_output_utf8) && start->width > 0 &&
         start->height > 0 && start->refresh_millihertz > 0 &&
         start->bitrate_kbps > 0 && start->codec >= 1 && start->codec <= 3 &&
         start->dynamic_range >= 1 && start->dynamic_range <= 2 &&
         valid_port_namespace(*start);
}

bool valid_launch_material(
    const CapyIoDisplayEngineLaunchMaterialV1 *material) noexcept {
  if (material == nullptr || material->struct_size != sizeof(*material) ||
      material->abi_version != CAPYIO_ENGINE_AUTHENTICATED_START_ABI_V1 ||
      material->launch_session_id == 0 || !nonzero_identity(material->key)) {
    return false;
  }
  for (const auto value : material->reserved) {
    if (value != 0) {
      return false;
    }
  }
  return true;
}

int32_t CAPYIO_ENGINE_CALL bridge_probe(CapyIoDisplayEngineProbeV2 *probe) {
  if (probe == nullptr || probe->struct_size != sizeof(*probe) ||
      probe->abi_version != CAPYIO_DISPLAY_ENGINE_ABI_V2) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }

#ifdef CAPYIO_VIDEO_ONLY
  probe->enabled_features = lifecycle_ready ? enabled_video_features : 0;
#else
  // The standalone ABI fixture has no retained capture/stream implementation.
  probe->enabled_features = 0;
#endif
  probe->max_concurrent_sessions = 1;
  probe->reserved = 0;
  return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL bridge_create(
    const CapyIoDisplayEngineCallbacksV2 *callbacks,
    void **output) {
  if (callbacks == nullptr || output == nullptr || callbacks->on_event == nullptr ||
      callbacks->struct_size != sizeof(*callbacks) ||
      callbacks->abi_version != CAPYIO_DISPLAY_ENGINE_ABI_V2) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }

  *output = nullptr;
  auto *engine = new (std::nothrow) EngineHandle{};
  if (engine == nullptr) {
    return CAPYIO_ENGINE_E_INTERNAL;
  }
#ifdef CAPYIO_VIDEO_ONLY
  engine->upstream_runtime =
      capyio::display_engine::create_upstream_runtime();
  if (engine->upstream_runtime == nullptr) {
    delete engine;
    return CAPYIO_ENGINE_E_INTERNAL;
  }
  engine->bootstrap = new (std::nothrow) capyio::display_engine::Bootstrap {
      capyio::display_engine::upstream_bootstrap_hooks(
          engine->upstream_runtime)};
  if (engine->bootstrap == nullptr) {
    (void) capyio::display_engine::destroy_upstream_runtime(
        engine->upstream_runtime);
    delete engine;
    return CAPYIO_ENGINE_E_INTERNAL;
  }
#endif
  engine->callbacks = *callbacks;
  *output = engine;
  return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL bridge_start(
    void *opaque,
    const CapyIoDisplayEngineStartV2 *start) {
  if (opaque == nullptr || !valid_start_config(start)) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }

  auto &engine = *static_cast<EngineHandle *>(opaque);
  if (engine.start_config.struct_size != 0) {
    return CAPYIO_ENGINE_E_INVALID_STATE;
  }

  // Own the complete pointer-free start record before any retained worker can
  // observe it. The output override lease is installed only in the private
  // video-only target and belongs to this handle.
  engine.start_config = *start;
#ifdef CAPYIO_VIDEO_ONLY
  engine.output_override_lease =
      config::acquire_capyio_display_session_config(
          reinterpret_cast<const char *>(
              engine.start_config.capture_output_utf8),
          static_cast<std::uint16_t>(engine.start_config.rtsp_port - 21));
  if (engine.output_override_lease == 0) {
    clear_start_config(engine);
    return CAPYIO_ENGINE_E_CAPACITY;
  }
#endif

  // Production never falls back to a launch without authenticated per-session
  // material, even though the frozen v2 table must retain this entry point.
  clear_start_config(engine);
  return CAPYIO_ENGINE_E_UNSUPPORTED;
}

int32_t bridge_start_authenticated(
    void *opaque,
    const CapyIoDisplayEngineStartV2 *start,
    const CapyIoDisplayEngineLaunchMaterialV1 *material) noexcept {
  if (opaque == nullptr || !valid_start_config(start) ||
      !valid_launch_material(material)) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }

  auto &engine = *static_cast<EngineHandle *>(opaque);
  if (engine.start_config.struct_size != 0) {
    return CAPYIO_ENGINE_E_INVALID_STATE;
  }
  engine.start_config = *start;
#ifdef CAPYIO_VIDEO_ONLY
  engine.output_override_lease =
      config::acquire_capyio_display_session_config(
          reinterpret_cast<const char *>(engine.start_config.capture_output_utf8),
          static_cast<std::uint16_t>(engine.start_config.rtsp_port - 21));
  if (engine.output_override_lease == 0) {
    clear_start_config(engine);
    return CAPYIO_ENGINE_E_CAPACITY;
  }

  capyio::display_engine::LaunchSessionMaterial owned_material {};
  owned_material.launch_session_id = material->launch_session_id;
  std::memcpy(owned_material.key.data(), material->key,
              owned_material.key.size());
  owned_material.key_id = material->key_id;
  std::memcpy(owned_material.ping_payload.data(), material->ping_payload,
              owned_material.ping_payload.size());
  owned_material.control_connect_data = material->control_connect_data;
  const auto staged = capyio::display_engine::stage_upstream_launch_session(
      engine.upstream_runtime, engine.start_config, owned_material);
  volatile std::uint8_t *secret = reinterpret_cast<volatile std::uint8_t *>(
      owned_material.key.data());
  for (std::size_t index = 0; index < owned_material.key.size(); ++index) {
    secret[index] = 0;
  }
  if (staged != capyio::display_engine::LaunchSessionResult::ok) {
    clear_start_config(engine);
    return staged == capyio::display_engine::LaunchSessionResult::invalid_state
               ? CAPYIO_ENGINE_E_INVALID_STATE
               : CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }
  if (!lifecycle_ready) {
    capyio::display_engine::discard_staged_upstream_launch_session(
        engine.upstream_runtime);
    clear_start_config(engine);
    return CAPYIO_ENGINE_E_UNSUPPORTED;
  }
  const auto started = engine.bootstrap->start(5000);
  if (started != capyio::display_engine::BootstrapResult::ok) {
    if (engine.bootstrap->state() ==
        capyio::display_engine::BootstrapState::idle) {
      capyio::display_engine::discard_staged_upstream_launch_session(
          engine.upstream_runtime);
      clear_start_config(engine);
    }
    return bootstrap_status(started);
  }
  return CAPYIO_ENGINE_OK;
#else
  clear_start_config(engine);
  return CAPYIO_ENGINE_E_UNSUPPORTED;
#endif
}

int32_t CAPYIO_ENGINE_CALL bridge_request_key_frame(void *opaque) {
  if (opaque == nullptr) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }
#ifdef CAPYIO_VIDEO_ONLY
  auto &engine = *static_cast<EngineHandle *>(opaque);
  if (engine.start_config.struct_size == 0 ||
      engine.bootstrap->state() !=
          capyio::display_engine::BootstrapState::running) {
    return CAPYIO_ENGINE_E_INVALID_STATE;
  }
  return capyio::display_engine::request_upstream_key_frame(
             engine.upstream_runtime)
             ? CAPYIO_ENGINE_OK
             : CAPYIO_ENGINE_E_INTERNAL;
#else
  return CAPYIO_ENGINE_E_INVALID_STATE;
#endif
}

int32_t CAPYIO_ENGINE_CALL bridge_stop(void *opaque, uint32_t timeout_millis) {
  if (opaque == nullptr || !valid_timeout(timeout_millis)) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }
#ifdef CAPYIO_VIDEO_ONLY
  auto &engine = *static_cast<EngineHandle *>(opaque);
  if (engine.start_config.struct_size == 0 ||
      engine.bootstrap->state() ==
          capyio::display_engine::BootstrapState::idle) {
    return CAPYIO_ENGINE_E_INVALID_STATE;
  }
  const auto stopped = engine.bootstrap->stop(timeout_millis);
  if (stopped != capyio::display_engine::BootstrapResult::ok) {
    return bootstrap_status(stopped);
  }
  clear_start_config(engine);
  return CAPYIO_ENGINE_OK;
#else
  return CAPYIO_ENGINE_E_INVALID_STATE;
#endif
}

int32_t CAPYIO_ENGINE_CALL bridge_destroy(void *opaque, uint32_t timeout_millis) {
  if (opaque == nullptr || !valid_timeout(timeout_millis)) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }
  auto *engine = static_cast<EngineHandle *>(opaque);
#ifdef CAPYIO_VIDEO_ONLY
  if (engine->bootstrap->state() !=
      capyio::display_engine::BootstrapState::idle) {
    const auto result = engine->bootstrap->stop(timeout_millis);
    if (result == capyio::display_engine::BootstrapResult::timeout) {
      return CAPYIO_ENGINE_E_TIMEOUT;
    }
    if (result != capyio::display_engine::BootstrapResult::ok) {
      return CAPYIO_ENGINE_E_INTERNAL;
    }
  }
  if (!capyio::display_engine::destroy_upstream_runtime(
          engine->upstream_runtime)) {
    return CAPYIO_ENGINE_E_INTERNAL;
  }
  engine->upstream_runtime = nullptr;
  delete engine->bootstrap;
  engine->bootstrap = nullptr;
#endif
  clear_start_config(*engine);
  delete engine;
  return CAPYIO_ENGINE_OK;
}

}  // namespace

CAPYIO_ENGINE_EXPORT int32_t CAPYIO_ENGINE_CALL
capyio_display_engine_get_api_v2(CapyIoDisplayEngineApiV2 *api) {
  if (api == nullptr || api->struct_size != sizeof(*api) ||
      api->abi_version != CAPYIO_DISPLAY_ENGINE_ABI_V2) {
    return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
  }

  api->probe = bridge_probe;
  api->create = bridge_create;
  api->start = bridge_start;
  api->request_key_frame = bridge_request_key_frame;
  api->stop = bridge_stop;
  api->destroy = bridge_destroy;
  for (void *&reserved : api->reserved) {
    reserved = nullptr;
  }
  return CAPYIO_ENGINE_OK;
}

CAPYIO_ENGINE_EXPORT int32_t CAPYIO_ENGINE_CALL
capyio_display_engine_start_authenticated_v1(
    void *engine,
    const CapyIoDisplayEngineStartV2 *start,
    const CapyIoDisplayEngineLaunchMaterialV1 *material) {
  return bridge_start_authenticated(engine, start, material);
}
