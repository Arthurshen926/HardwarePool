#include "../../include/capyio_display_engine_v1.h"

#include <windows.h>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <iostream>

namespace {

std::atomic<unsigned> callback_count {0};

void CAPYIO_ENGINE_CALL on_event(
    void *, const CapyIoDisplayEngineEventV2 *) {
  callback_count.fetch_add(1, std::memory_order_relaxed);
}

bool expect(bool condition, const char *message) {
  if (!condition) {
    std::cerr << message << '\n';
  }
  return condition;
}

}  // namespace

int wmain(int argc, wchar_t **argv) {
  if (argc != 2) {
    return 2;
  }
  HMODULE module = LoadLibraryExW(argv[1], nullptr,
                                  LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                      LOAD_LIBRARY_SEARCH_SYSTEM32);
  if (!expect(module != nullptr, "failed to load bridge DLL")) {
    return 3;
  }
  auto get_api = reinterpret_cast<decltype(&capyio_display_engine_get_api_v2)>(
      GetProcAddress(module, "capyio_display_engine_get_api_v2"));
  auto start_authenticated = reinterpret_cast<
      decltype(&capyio_display_engine_start_authenticated_v1)>(
      GetProcAddress(
          module, "capyio_display_engine_start_authenticated_v1"));
  if (!expect(get_api != nullptr && start_authenticated != nullptr,
              "missing ABI export")) {
    FreeLibrary(module);
    return 4;
  }

  CapyIoDisplayEngineApiV2 api = {};
  api.struct_size = sizeof(api);
  api.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
  bool ok = expect(get_api(&api) == CAPYIO_ENGINE_OK, "get_api failed");
  ok &= expect(api.probe && api.create && api.start && api.request_key_frame &&
                   api.stop && api.destroy,
               "function table is incomplete");

  CapyIoDisplayEngineProbeV2 probe = {};
  probe.struct_size = sizeof(probe);
  probe.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
  ok &= expect(api.probe(&probe) == CAPYIO_ENGINE_OK, "probe failed");
  ok &= expect(probe.enabled_features == 0, "intermediate bridge claimed features");
  ok &= expect(probe.max_concurrent_sessions == 1 && probe.reserved == 0,
               "probe bounds are incorrect");

  CapyIoDisplayEngineCallbacksV2 callbacks = {};
  callbacks.struct_size = sizeof(callbacks);
  callbacks.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
  callbacks.on_event = on_event;
  void *engine = nullptr;
  ok &= expect(api.create(&callbacks, &engine) == CAPYIO_ENGINE_OK && engine,
               "create failed");

  CapyIoDisplayEngineStartV2 start = {};
  start.struct_size = sizeof(start);
  start.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
  start.adapter_instance_id[0] = 1;
  start.display_capability_id[0] = 2;
  std::memcpy(start.capture_output_utf8, "\\\\.\\display2", 12);
  start.width = 2640;
  start.height = 1216;
  start.refresh_millihertz = 120000;
  start.codec = 2;
  start.dynamic_range = 1;
  start.bitrate_kbps = 45000;
  start.https_port = 49995;
  start.video_port = 50009;
  start.control_port = 50010;
  start.rtsp_port = 50021;
  ok &= expect(api.start(engine, &start) == CAPYIO_ENGINE_E_INVALID_ARGUMENT,
               "non-canonical capture output was accepted");
  std::memset(start.capture_output_utf8, 0, sizeof(start.capture_output_utf8));
  std::memcpy(start.capture_output_utf8, "\\\\.\\DISPLAY2", 12);
  ++start.https_port;
  ok &= expect(api.start(engine, &start) == CAPYIO_ENGINE_E_INVALID_ARGUMENT,
               "inconsistent GameStream port namespace was accepted");
  --start.https_port;
  CapyIoDisplayEngineLaunchMaterialV1 material = {};
  material.struct_size = sizeof(material);
  material.abi_version = CAPYIO_ENGINE_AUTHENTICATED_START_ABI_V1;
  material.launch_session_id = 7;
  material.key_id = 0x12345678;
  material.ping_payload[0] = 1;
  material.control_connect_data = 9;
  ok &= expect(
      start_authenticated(engine, &start, &material) ==
          CAPYIO_ENGINE_E_INVALID_ARGUMENT,
      "all-zero authenticated session key was accepted");
  material.key[0] = 0x42;
  material.reserved[0] = 1;
  ok &= expect(
      start_authenticated(engine, &start, &material) ==
          CAPYIO_ENGINE_E_INVALID_ARGUMENT,
      "nonzero authenticated-start reserved field was accepted");
  material.reserved[0] = 0;
  ok &= expect(
      start_authenticated(engine, &start, &material) ==
          CAPYIO_ENGINE_E_UNSUPPORTED,
      "authenticated start handoff did not remain fail closed");
  ok &= expect(api.start(engine, &start) == CAPYIO_ENGINE_E_UNSUPPORTED,
               "unwired bridge did not fail closed");
  std::memset(start.capture_output_utf8, 0, sizeof(start.capture_output_utf8));
  std::memcpy(start.capture_output_utf8, "\\\\.\\DISPLAY3", 12);
  ok &= expect(api.start(engine, &start) == CAPYIO_ENGINE_E_UNSUPPORTED,
               "failed start retained the prior instance configuration");
  ok &= expect(api.request_key_frame(engine) == CAPYIO_ENGINE_E_INVALID_STATE,
               "key-frame request accepted without a session");
  ok &= expect(api.stop(engine, 1000) == CAPYIO_ENGINE_E_INVALID_STATE,
               "stop accepted without a session");
  ok &= expect(api.destroy(engine, 1000) == CAPYIO_ENGINE_OK, "destroy failed");
  ok &= expect(callback_count.load(std::memory_order_relaxed) == 0,
               "unstarted bridge emitted a callback");

  FreeLibrary(module);
  return ok ? 0 : 5;
}
