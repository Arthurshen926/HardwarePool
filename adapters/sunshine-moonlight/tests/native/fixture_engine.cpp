#define CAPYIO_DISPLAY_ENGINE_EXPORTS
#include "../../include/capyio_display_engine_v1.h"

#include <atomic>
#include <chrono>
#include <cstring>
#include <new>
#include <thread>

namespace {

struct FixtureEngine {
    CapyIoDisplayEngineCallbacksV2 callbacks;
    std::atomic<bool> running;
    uint8_t adapter_instance_id[16];
    uint8_t display_capability_id[16];
    std::atomic<uint64_t> sequence;
    std::thread worker;
};

bool nonzero(const uint8_t value[16]) {
    for (unsigned index = 0; index < 16; ++index) {
        if (value[index] != 0) {
            return true;
        }
    }
    return false;
}

bool valid_launch_material(
    const CapyIoDisplayEngineLaunchMaterialV1 *material) {
    if (material == nullptr || material->struct_size != sizeof(*material) ||
        material->abi_version != CAPYIO_ENGINE_AUTHENTICATED_START_ABI_V1 ||
        material->launch_session_id == 0 || !nonzero(material->key)) {
        return false;
    }
    for (const auto value : material->reserved) {
        if (value != 0) {
            return false;
        }
    }
    return true;
}

bool valid_capture_output(
    const uint8_t value[CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY]) {
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

void emit(FixtureEngine *engine, uint32_t kind, int32_t status, uint64_t value) {
    CapyIoDisplayEngineEventV2 event = {};
    event.struct_size = sizeof(event);
    event.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
    event.kind = kind;
    event.status = status;
    std::memcpy(event.adapter_instance_id, engine->adapter_instance_id, 16);
    std::memcpy(event.display_capability_id, engine->display_capability_id, 16);
    event.sequence = engine->sequence.fetch_add(1, std::memory_order_relaxed) + 1;
    event.value = value;
    engine->callbacks.on_event(engine->callbacks.context, &event);
}

int32_t CAPYIO_ENGINE_CALL fixture_probe(CapyIoDisplayEngineProbeV2 *probe) {
    if (probe == nullptr || probe->struct_size != sizeof(*probe) ||
        probe->abi_version != CAPYIO_DISPLAY_ENGINE_ABI_V2) {
        return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    }
    probe->enabled_features = CAPYIO_ENGINE_FEATURE_CAPTURE |
                              CAPYIO_ENGINE_FEATURE_HARDWARE_ENCODE |
                              CAPYIO_ENGINE_FEATURE_PACING |
                              CAPYIO_ENGINE_FEATURE_PACKET_RECOVERY |
                              CAPYIO_ENGINE_FEATURE_VIDEO_CONTROL |
                              CAPYIO_ENGINE_FEATURE_HDR10 |
                              CAPYIO_ENGINE_FEATURE_HEVC;
    probe->max_concurrent_sessions = 1;
    probe->reserved = 0;
    return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL fixture_create(
    const CapyIoDisplayEngineCallbacksV2 *callbacks,
    void **output) {
    if (callbacks == nullptr || output == nullptr || callbacks->on_event == nullptr ||
        callbacks->struct_size != sizeof(*callbacks) ||
        callbacks->abi_version != CAPYIO_DISPLAY_ENGINE_ABI_V2) {
        return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    }
    FixtureEngine *engine = new (std::nothrow) FixtureEngine{};
    if (engine == nullptr) {
        return CAPYIO_ENGINE_E_INTERNAL;
    }
    engine->callbacks = *callbacks;
    *output = engine;
    return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL fixture_start(
    void *opaque,
    const CapyIoDisplayEngineStartV2 *start) {
    auto *engine = static_cast<FixtureEngine *>(opaque);
    if (engine == nullptr || start == nullptr || start->struct_size != sizeof(*start) ||
        start->abi_version != CAPYIO_DISPLAY_ENGINE_ABI_V2 ||
        engine->running.load(std::memory_order_acquire) ||
        !nonzero(start->adapter_instance_id) || !nonzero(start->display_capability_id) ||
        !valid_capture_output(start->capture_output_utf8) || start->rtsp_port < 21 ||
        start->https_port != start->rtsp_port - 26 ||
        start->video_port != start->rtsp_port - 12 ||
        start->control_port != start->rtsp_port - 11) {
        return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    }
    std::memcpy(engine->adapter_instance_id, start->adapter_instance_id, 16);
    std::memcpy(engine->display_capability_id, start->display_capability_id, 16);
    engine->sequence.store(0, std::memory_order_relaxed);
    engine->running.store(true, std::memory_order_release);
    emit(engine, CAPYIO_ENGINE_EVENT_STARTED, CAPYIO_ENGINE_OK, 0);
    emit(engine, CAPYIO_ENGINE_EVENT_FIRST_FRAME, CAPYIO_ENGINE_OK, 16667);
    engine->worker = std::thread([engine]() {
        uint64_t pressure = 1;
        while (engine->running.load(std::memory_order_acquire)) {
            emit(engine, CAPYIO_ENGINE_EVENT_CONGESTION_CHANGED,
                 CAPYIO_ENGINE_OK, pressure++);
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        }
    });
    return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL fixture_request_key_frame(void *opaque) {
    auto *engine = static_cast<FixtureEngine *>(opaque);
    if (engine == nullptr || !engine->running.load(std::memory_order_acquire)) {
        return CAPYIO_ENGINE_E_INVALID_STATE;
    }
    emit(engine, CAPYIO_ENGINE_EVENT_KEY_FRAME_REQUESTED, CAPYIO_ENGINE_OK, 0);
    return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL fixture_stop(void *opaque, uint32_t timeout_millis) {
    auto *engine = static_cast<FixtureEngine *>(opaque);
    if (engine == nullptr || !engine->running.load(std::memory_order_acquire)) {
        return CAPYIO_ENGINE_E_INVALID_STATE;
    }
    if (timeout_millis == 100) {
        return CAPYIO_ENGINE_E_TIMEOUT;
    }
    if (timeout_millis < 100 || timeout_millis > 10000) {
        return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    }
    engine->running.store(false, std::memory_order_release);
    if (engine->worker.joinable()) {
        engine->worker.join();
    }
    emit(engine, CAPYIO_ENGINE_EVENT_STOPPED, CAPYIO_ENGINE_OK, 0);
    return CAPYIO_ENGINE_OK;
}

int32_t CAPYIO_ENGINE_CALL fixture_destroy(void *opaque, uint32_t timeout_millis) {
    auto *engine = static_cast<FixtureEngine *>(opaque);
    if (engine == nullptr || engine->running.load(std::memory_order_acquire) ||
        engine->worker.joinable()) {
        return CAPYIO_ENGINE_E_INVALID_STATE;
    }
    if (timeout_millis < 100 || timeout_millis > 10000) {
        return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    }
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
    api->probe = fixture_probe;
    api->create = fixture_create;
    api->start = fixture_start;
    api->request_key_frame = fixture_request_key_frame;
    api->stop = fixture_stop;
    api->destroy = fixture_destroy;
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
    if (!valid_launch_material(material)) {
        return CAPYIO_ENGINE_E_INVALID_ARGUMENT;
    }
    return fixture_start(engine, start);
}
