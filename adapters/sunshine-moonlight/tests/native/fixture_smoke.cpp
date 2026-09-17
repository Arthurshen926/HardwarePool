#include "../../include/capyio_display_engine_v1.h"

#include <Windows.h>

#include <atomic>
#include <cstdio>
#include <cstring>

namespace {

struct Events {
    std::atomic<uint32_t> count;
    std::atomic<uint32_t> last_kind;
    std::atomic<uint64_t> last_sequence;
    std::atomic<bool> identity_matches;
};

void CAPYIO_ENGINE_CALL on_event(
    void *context,
    const CapyIoDisplayEngineEventV2 *event) {
    auto *events = static_cast<Events *>(context);
    if (event == nullptr || event->struct_size != sizeof(*event) || event->sequence == 0) {
        events->identity_matches.store(false, std::memory_order_relaxed);
        return;
    }
    uint8_t expected_adapter[16];
    uint8_t expected_display[16];
    std::memset(expected_adapter, 3, sizeof(expected_adapter));
    std::memset(expected_display, 4, sizeof(expected_display));
    const bool identity_matches =
        std::memcmp(event->adapter_instance_id, expected_adapter, 16) == 0 &&
        std::memcmp(event->display_capability_id, expected_display, 16) == 0;
    if (!identity_matches) {
        events->identity_matches.store(false, std::memory_order_relaxed);
    }
    events->count.fetch_add(1, std::memory_order_relaxed);
    events->last_kind.store(event->kind, std::memory_order_relaxed);
    events->last_sequence.store(event->sequence, std::memory_order_relaxed);
}

int fail(const char *message) {
    std::fprintf(stderr, "fixture smoke failed: %s\n", message);
    return 1;
}

}  // namespace

int wmain(int argc, wchar_t **argv) {
    if (argc != 2) {
        return fail("expected one absolute DLL path");
    }
    HMODULE module = LoadLibraryExW(
        argv[1], nullptr,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (module == nullptr) {
        return fail("LoadLibraryExW");
    }
    auto get_api = reinterpret_cast<decltype(&capyio_display_engine_get_api_v2)>(
        GetProcAddress(module, "capyio_display_engine_get_api_v2"));
    if (get_api == nullptr) {
        return fail("GetProcAddress");
    }
    CapyIoDisplayEngineApiV2 api = {};
    api.struct_size = sizeof(api);
    api.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
    if (get_api(&api) != CAPYIO_ENGINE_OK) {
        return fail("get_api");
    }
    CapyIoDisplayEngineProbeV2 probe = {};
    probe.struct_size = sizeof(probe);
    probe.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
    if (api.probe(&probe) != CAPYIO_ENGINE_OK || probe.max_concurrent_sessions != 1) {
        return fail("probe");
    }

    Events events = {};
    events.identity_matches.store(true, std::memory_order_relaxed);
    CapyIoDisplayEngineCallbacksV2 callbacks = {};
    callbacks.struct_size = sizeof(callbacks);
    callbacks.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
    callbacks.on_event = on_event;
    callbacks.context = &events;
    void *engine = nullptr;
    if (api.create(&callbacks, &engine) != CAPYIO_ENGINE_OK || engine == nullptr) {
        return fail("create");
    }
    CapyIoDisplayEngineStartV2 start = {};
    start.struct_size = sizeof(start);
    start.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
    std::memset(start.adapter_instance_id, 3, sizeof(start.adapter_instance_id));
    std::memset(start.display_capability_id, 4, sizeof(start.display_capability_id));
    std::memcpy(start.capture_output_utf8, "\\\\.\\DISPLAY02", 13);
    start.width = 2640;
    start.height = 1216;
    start.refresh_millihertz = 120000;
    start.codec = 2;
    start.dynamic_range = 2;
    start.bitrate_kbps = 45000;
    start.https_port = 49995;
    start.video_port = 50009;
    start.control_port = 50010;
    start.rtsp_port = 50021;
    if (api.start(engine, &start) != CAPYIO_ENGINE_E_INVALID_ARGUMENT) {
        return fail("non-canonical capture output accepted");
    }
    std::memset(start.capture_output_utf8, 0, sizeof(start.capture_output_utf8));
    std::memcpy(start.capture_output_utf8, "\\\\.\\DISPLAY2", 12);
    if (api.start(engine, &start) != CAPYIO_ENGINE_OK) {
        return fail("start events");
    }
    Sleep(10);
    const uint32_t after_start = events.count.load(std::memory_order_relaxed);
    if (after_start < 3 || api.request_key_frame(engine) != CAPYIO_ENGINE_OK) {
        return fail("key-frame event");
    }
    if (api.stop(engine, 100) != CAPYIO_ENGINE_E_TIMEOUT) {
        return fail("injected stop timeout");
    }
    if (api.request_key_frame(engine) != CAPYIO_ENGINE_OK) {
        return fail("ownership after timeout");
    }
    if (api.destroy(engine, 1000) != CAPYIO_ENGINE_E_INVALID_STATE) {
        return fail("destroy while running");
    }
    if (api.stop(engine, 1000) != CAPYIO_ENGINE_OK ||
        events.last_kind.load(std::memory_order_relaxed) != CAPYIO_ENGINE_EVENT_STOPPED ||
        !events.identity_matches.load(std::memory_order_relaxed)) {
        return fail("quiescent stop");
    }
    const uint32_t stopped_count = events.count.load(std::memory_order_relaxed);
    Sleep(20);
    if (events.count.load(std::memory_order_relaxed) != stopped_count) {
        return fail("callback after stop");
    }
    if (api.destroy(engine, 1000) != CAPYIO_ENGINE_OK ||
        events.count.load(std::memory_order_relaxed) != stopped_count) {
        return fail("destroy or callback after stop");
    }
    if (!FreeLibrary(module)) {
        return fail("FreeLibrary");
    }
    std::printf("fixture smoke passed: events=%u timeout_preserved_owner=true\n", stopped_count);
    return 0;
}
