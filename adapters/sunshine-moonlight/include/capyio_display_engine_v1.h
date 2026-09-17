#ifndef CAPYIO_DISPLAY_ENGINE_V1_H
#define CAPYIO_DISPLAY_ENGINE_V1_H

#include <stdint.h>

#define CAPYIO_DISPLAY_ENGINE_ABI_V1 1u
#define CAPYIO_DISPLAY_ENGINE_ABI_V2 2u
#define CAPYIO_ENGINE_AUTHENTICATED_START_ABI_V1 1u
#define CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY 64u

#define CAPYIO_ENGINE_FEATURE_CAPTURE (UINT64_C(1) << 0)
#define CAPYIO_ENGINE_FEATURE_HARDWARE_ENCODE (UINT64_C(1) << 1)
#define CAPYIO_ENGINE_FEATURE_PACING (UINT64_C(1) << 2)
#define CAPYIO_ENGINE_FEATURE_PACKET_RECOVERY (UINT64_C(1) << 3)
#define CAPYIO_ENGINE_FEATURE_VIDEO_CONTROL (UINT64_C(1) << 4)
#define CAPYIO_ENGINE_FEATURE_HDR10 (UINT64_C(1) << 5)
#define CAPYIO_ENGINE_FEATURE_HEVC (UINT64_C(1) << 6)
#define CAPYIO_ENGINE_FEATURE_AV1 (UINT64_C(1) << 7)

#define CAPYIO_ENGINE_FEATURE_AUDIO (UINT64_C(1) << 32)
#define CAPYIO_ENGINE_FEATURE_INPUT (UINT64_C(1) << 33)
#define CAPYIO_ENGINE_FEATURE_GAMEPAD (UINT64_C(1) << 34)
#define CAPYIO_ENGINE_FEATURE_DISCOVERY (UINT64_C(1) << 35)
#define CAPYIO_ENGINE_FEATURE_UPNP (UINT64_C(1) << 36)
#define CAPYIO_ENGINE_FEATURE_GLOBAL_TOPOLOGY (UINT64_C(1) << 37)
#define CAPYIO_ENGINE_FEATURE_FOREIGN_MONITOR_CLEANUP (UINT64_C(1) << 38)

#define CAPYIO_ENGINE_OK 0
#define CAPYIO_ENGINE_E_INVALID_ARGUMENT -1
#define CAPYIO_ENGINE_E_INVALID_STATE -2
#define CAPYIO_ENGINE_E_CAPACITY -3
#define CAPYIO_ENGINE_E_UNSUPPORTED -4
#define CAPYIO_ENGINE_E_INTERNAL -5
#define CAPYIO_ENGINE_E_TIMEOUT -6

#define CAPYIO_ENGINE_EVENT_STARTED 1u
#define CAPYIO_ENGINE_EVENT_FIRST_FRAME 2u
#define CAPYIO_ENGINE_EVENT_KEY_FRAME_REQUESTED 3u
#define CAPYIO_ENGINE_EVENT_CONGESTION_CHANGED 4u
#define CAPYIO_ENGINE_EVENT_STOPPED 5u
#define CAPYIO_ENGINE_EVENT_FATAL 6u

#if defined(_WIN32)
#define CAPYIO_ENGINE_CALL __cdecl
#if defined(CAPYIO_DISPLAY_ENGINE_EXPORTS)
#define CAPYIO_ENGINE_EXPORT __declspec(dllexport)
#else
#define CAPYIO_ENGINE_EXPORT __declspec(dllimport)
#endif
#else
#define CAPYIO_ENGINE_CALL
#define CAPYIO_ENGINE_EXPORT
#endif

typedef struct CapyIoDisplayEngineProbeV1 {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t enabled_features;
    uint32_t max_concurrent_sessions;
    uint32_t reserved;
} CapyIoDisplayEngineProbeV1;

typedef struct CapyIoDisplayEngineStartV1 {
    uint32_t struct_size;
    uint32_t abi_version;
    uint8_t adapter_instance_id[16];
    uint8_t display_capability_id[16];
    uint32_t width;
    uint32_t height;
    uint32_t refresh_millihertz;
    uint32_t codec;
    uint32_t dynamic_range;
    uint32_t bitrate_kbps;
    uint16_t https_port;
    uint16_t video_port;
    uint16_t control_port;
    uint16_t rtsp_port;
} CapyIoDisplayEngineStartV1;

typedef struct CapyIoDisplayEngineEventV1 {
    uint32_t struct_size;
    uint32_t abi_version;
    uint32_t kind;
    int32_t status;
    uint8_t adapter_instance_id[16];
    uint8_t display_capability_id[16];
    uint64_t sequence;
    uint64_t value;
} CapyIoDisplayEngineEventV1;

typedef void(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineEventCallbackV1)(
    void *context,
    const CapyIoDisplayEngineEventV1 *event);

typedef struct CapyIoDisplayEngineCallbacksV1 {
    uint32_t struct_size;
    uint32_t abi_version;
    CapyIoDisplayEngineEventCallbackV1 on_event;
    void *context;
} CapyIoDisplayEngineCallbacksV1;

typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineProbeFnV1)(
    CapyIoDisplayEngineProbeV1 *probe);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineCreateFnV1)(
    const CapyIoDisplayEngineCallbacksV1 *callbacks,
    void **engine);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineStartFnV1)(
    void *engine,
    const CapyIoDisplayEngineStartV1 *start);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineRequestKeyFrameFnV1)(
    void *engine);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineStopFnV1)(
    void *engine,
    uint32_t timeout_millis);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineDestroyFnV1)(
    void *engine,
    uint32_t timeout_millis);

typedef struct CapyIoDisplayEngineApiV1 {
    uint32_t struct_size;
    uint32_t abi_version;
    CapyIoDisplayEngineProbeFnV1 probe;
    CapyIoDisplayEngineCreateFnV1 create;
    CapyIoDisplayEngineStartFnV1 start;
    CapyIoDisplayEngineRequestKeyFrameFnV1 request_key_frame;
    CapyIoDisplayEngineStopFnV1 stop;
    CapyIoDisplayEngineDestroyFnV1 destroy;
    void *reserved[4];
} CapyIoDisplayEngineApiV1;

#if defined(__cplusplus)
extern "C" {
#endif

CAPYIO_ENGINE_EXPORT int32_t CAPYIO_ENGINE_CALL
capyio_display_engine_get_api_v1(CapyIoDisplayEngineApiV1 *api);

/*
 * ABI v2 retains the bounded probe, callback, event and function-table shapes
 * but adds an inline exact Windows capture output to the start record. V1
 * remains fail-closed and its reserved storage is never repurposed.
 */
typedef CapyIoDisplayEngineProbeV1 CapyIoDisplayEngineProbeV2;
typedef CapyIoDisplayEngineEventV1 CapyIoDisplayEngineEventV2;
typedef CapyIoDisplayEngineCallbacksV1 CapyIoDisplayEngineCallbacksV2;

typedef struct CapyIoDisplayEngineStartV2 {
    uint32_t struct_size;
    uint32_t abi_version;
    uint8_t adapter_instance_id[16];
    uint8_t display_capability_id[16];
    uint8_t capture_output_utf8[CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY];
    uint32_t width;
    uint32_t height;
    uint32_t refresh_millihertz;
    uint32_t codec;
    uint32_t dynamic_range;
    uint32_t bitrate_kbps;
    uint16_t https_port;
    uint16_t video_port;
    uint16_t control_port;
    uint16_t rtsp_port;
} CapyIoDisplayEngineStartV2;

/*
 * One-shot material established only after the receiver and Route have been
 * authenticated. It is submitted atomically with StartV2 through the separate
 * authenticated-start extension; it must never be logged or persisted.
 */
typedef struct CapyIoDisplayEngineLaunchMaterialV1 {
    uint32_t struct_size;
    uint32_t abi_version;
    uint32_t launch_session_id;
    uint8_t key[16];
    uint32_t key_id;
    uint8_t ping_payload[8];
    uint32_t control_connect_data;
    uint32_t reserved[4];
} CapyIoDisplayEngineLaunchMaterialV1;

typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineProbeFnV2)(
    CapyIoDisplayEngineProbeV2 *probe);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineCreateFnV2)(
    const CapyIoDisplayEngineCallbacksV2 *callbacks,
    void **engine);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineStartFnV2)(
    void *engine,
    const CapyIoDisplayEngineStartV2 *start);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineRequestKeyFrameFnV2)(
    void *engine);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineStopFnV2)(
    void *engine,
    uint32_t timeout_millis);
typedef int32_t(CAPYIO_ENGINE_CALL *CapyIoDisplayEngineDestroyFnV2)(
    void *engine,
    uint32_t timeout_millis);

typedef struct CapyIoDisplayEngineApiV2 {
    uint32_t struct_size;
    uint32_t abi_version;
    CapyIoDisplayEngineProbeFnV2 probe;
    CapyIoDisplayEngineCreateFnV2 create;
    CapyIoDisplayEngineStartFnV2 start;
    CapyIoDisplayEngineRequestKeyFrameFnV2 request_key_frame;
    CapyIoDisplayEngineStopFnV2 stop;
    CapyIoDisplayEngineDestroyFnV2 destroy;
    void *reserved[4];
} CapyIoDisplayEngineApiV2;

CAPYIO_ENGINE_EXPORT int32_t CAPYIO_ENGINE_CALL
capyio_display_engine_get_api_v2(CapyIoDisplayEngineApiV2 *api);

/*
 * ABI v2's function table stays frozen. Engines that accept real session
 * material expose this additional symbol; callers must not fall back to the
 * unauthenticated StartV2 function when it is absent.
 */
CAPYIO_ENGINE_EXPORT int32_t CAPYIO_ENGINE_CALL
capyio_display_engine_start_authenticated_v1(
    void *engine,
    const CapyIoDisplayEngineStartV2 *start,
    const CapyIoDisplayEngineLaunchMaterialV1 *material);

#if defined(__cplusplus)
}
#endif

#if defined(__cplusplus)
static_assert(sizeof(CapyIoDisplayEngineProbeV1) == 24,
              "CapyIO display engine probe ABI drift");
static_assert(sizeof(CapyIoDisplayEngineStartV1) == 72,
              "CapyIO display engine start ABI drift");
static_assert(sizeof(CapyIoDisplayEngineStartV2) == 136,
              "CapyIO display engine v2 start ABI drift");
static_assert(sizeof(CapyIoDisplayEngineLaunchMaterialV1) == 60,
              "CapyIO authenticated launch material ABI drift");
static_assert(sizeof(CapyIoDisplayEngineEventV1) == 64,
              "CapyIO display engine event ABI drift");
static_assert(sizeof(CapyIoDisplayEngineEventV2) == 64,
              "CapyIO display engine v2 event ABI drift");
#if INTPTR_MAX == INT64_MAX
static_assert(sizeof(CapyIoDisplayEngineCallbacksV1) == 24,
              "CapyIO display engine callback ABI drift");
static_assert(sizeof(CapyIoDisplayEngineApiV1) == 88,
              "CapyIO display engine function table ABI drift");
static_assert(sizeof(CapyIoDisplayEngineCallbacksV2) == 24,
              "CapyIO display engine v2 callback ABI drift");
static_assert(sizeof(CapyIoDisplayEngineApiV2) == 88,
              "CapyIO display engine v2 function table ABI drift");
#endif
#endif

#endif
