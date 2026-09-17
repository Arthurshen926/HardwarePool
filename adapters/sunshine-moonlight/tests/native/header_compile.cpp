#include "../../include/capyio_display_engine_v1.h"

static_assert(CAPYIO_DISPLAY_ENGINE_ABI_V2 == 2u);
static_assert(CAPYIO_ENGINE_AUTHENTICATED_START_ABI_V1 == 1u);
static_assert(CAPYIO_ENGINE_FEATURE_CAPTURE == (UINT64_C(1) << 0));
static_assert(CAPYIO_ENGINE_FEATURE_AUDIO == (UINT64_C(1) << 32));
static_assert(CAPYIO_ENGINE_E_TIMEOUT == -6);
static_assert(CAPYIO_ENGINE_EVENT_FATAL == 6u);

int capyio_display_engine_header_compile_probe() {
    CapyIoDisplayEngineProbeV2 probe = {};
    CapyIoDisplayEngineStartV2 start = {};
    CapyIoDisplayEngineLaunchMaterialV1 material = {};
    CapyIoDisplayEngineEventV2 event = {};
    CapyIoDisplayEngineCallbacksV2 callbacks = {};
    CapyIoDisplayEngineApiV2 api = {};
    return static_cast<int>(probe.struct_size + start.struct_size + material.struct_size + event.struct_size +
                            callbacks.struct_size + api.struct_size);
}
