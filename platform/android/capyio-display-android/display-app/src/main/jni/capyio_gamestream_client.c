#include <jni.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "Limelight.h"

#define CAPYIO_MAX_DECODE_UNIT_BYTES (16 * 1024 * 1024)
#define CAPYIO_INITIAL_DECODE_UNIT_BYTES (32 * 1024)

static JavaVM* g_vm;
static pthread_key_t g_attach_key;
static pthread_once_t g_attach_key_once = PTHREAD_ONCE_INIT;
static pthread_mutex_t g_state_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t g_state_cond = PTHREAD_COND_INITIALIZER;
static jobject g_receiver;
static jbyteArray g_decode_buffer;
static bool g_owned;
static bool g_stop_requested;
static bool g_terminated;
static int g_termination_error;

static jmethodID g_on_stage;
static jmethodID g_on_connected;
static jmethodID g_on_terminated;
static jmethodID g_on_video_setup;
static jmethodID g_on_video_start;
static jmethodID g_on_video_stop;
static jmethodID g_on_video_cleanup;
static jmethodID g_on_video_unit;

static void detach_thread(void* ignored) {
    (void)ignored;
    if (g_vm != NULL) {
        (*g_vm)->DetachCurrentThread(g_vm);
    }
}

static void create_attach_key(void) {
    (void)pthread_key_create(&g_attach_key, detach_thread);
}

static JNIEnv* get_env(void) {
    JNIEnv* env = NULL;
    if ((*g_vm)->GetEnv(g_vm, (void**)&env, JNI_VERSION_1_6) == JNI_OK) {
        return env;
    }
    if ((*g_vm)->AttachCurrentThread(g_vm, &env, NULL) != JNI_OK) {
        return NULL;
    }
    pthread_once(&g_attach_key_once, create_attach_key);
    (void)pthread_setspecific(g_attach_key, (void*)1);
    return env;
}

static void secure_zero(void* data, size_t length) {
    volatile unsigned char* bytes = (volatile unsigned char*)data;
    while (length-- != 0) {
        *bytes++ = 0;
    }
}

static bool clear_exception(JNIEnv* env) {
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        return true;
    }
    return false;
}

static void notify_stage_text(const char* text) {
    JNIEnv* env = get_env();
    if (env == NULL || g_receiver == NULL) {
        return;
    }
    jstring value = (*env)->NewStringUTF(env, text);
    if (value == NULL) {
        clear_exception(env);
        return;
    }
    (*env)->CallVoidMethod(env, g_receiver, g_on_stage, value);
    clear_exception(env);
    (*env)->DeleteLocalRef(env, value);
}

static void stage_starting(int stage) {
    const char* name = LiGetStageName(stage);
    char message[96];
    int written = snprintf(message, sizeof(message), "GameStream: %s", name != NULL ? name : "stage");
    if (written > 0 && (size_t)written < sizeof(message)) {
        notify_stage_text(message);
    }
}

static void stage_complete(int stage) {
    (void)stage;
}

static void stage_failed(int stage, int error_code) {
    char message[112];
    int written = snprintf(message, sizeof(message), "GameStream stage %d failed (%d)", stage, error_code);
    if (written > 0 && (size_t)written < sizeof(message)) {
        notify_stage_text(message);
    }
}

static void connection_started(void) {
    JNIEnv* env = get_env();
    pthread_mutex_lock(&g_state_mutex);
    pthread_mutex_unlock(&g_state_mutex);
    if (env != NULL && g_receiver != NULL) {
        (*env)->CallVoidMethod(env, g_receiver, g_on_connected);
        clear_exception(env);
    }
}

static void connection_terminated(int error_code) {
    JNIEnv* env = get_env();
    pthread_mutex_lock(&g_state_mutex);
    g_terminated = true;
    g_termination_error = error_code;
    pthread_cond_signal(&g_state_cond);
    pthread_mutex_unlock(&g_state_mutex);
    if (env != NULL && g_receiver != NULL) {
        (*env)->CallVoidMethod(env, g_receiver, g_on_terminated, (jint)error_code);
        clear_exception(env);
    }
}

static void discard_log(const char* format, ...) {
    (void)format;
}

static int video_setup(int video_format, int width, int height, int redraw_rate,
                       void* context, int flags) {
    (void)context;
    JNIEnv* env = get_env();
    if (env == NULL || g_receiver == NULL) {
        return -1;
    }
    jint result = (*env)->CallIntMethod(env, g_receiver, g_on_video_setup,
                                       video_format, width, height, redraw_rate, flags);
    if (clear_exception(env) || result != 0) {
        return -1;
    }
    if (g_decode_buffer != NULL) {
        (*env)->DeleteGlobalRef(env, g_decode_buffer);
    }
    jbyteArray local = (*env)->NewByteArray(env, CAPYIO_INITIAL_DECODE_UNIT_BYTES);
    if (local == NULL) {
        clear_exception(env);
        return -1;
    }
    g_decode_buffer = (*env)->NewGlobalRef(env, local);
    (*env)->DeleteLocalRef(env, local);
    return g_decode_buffer != NULL ? 0 : -1;
}

static void video_start(void) {
    JNIEnv* env = get_env();
    if (env != NULL && g_receiver != NULL) {
        (*env)->CallVoidMethod(env, g_receiver, g_on_video_start);
        clear_exception(env);
    }
}

static void video_stop(void) {
    JNIEnv* env = get_env();
    if (env != NULL && g_receiver != NULL) {
        (*env)->CallVoidMethod(env, g_receiver, g_on_video_stop);
        clear_exception(env);
    }
}

static void video_cleanup(void) {
    JNIEnv* env = get_env();
    if (env == NULL) {
        return;
    }
    if (g_decode_buffer != NULL) {
        (*env)->DeleteGlobalRef(env, g_decode_buffer);
        g_decode_buffer = NULL;
    }
    if (g_receiver != NULL) {
        (*env)->CallVoidMethod(env, g_receiver, g_on_video_cleanup);
        clear_exception(env);
    }
}

static int submit_one(JNIEnv* env, int length, int buffer_type,
                      PDECODE_UNIT unit) {
    jint result = (*env)->CallIntMethod(
        env, g_receiver, g_on_video_unit, g_decode_buffer, length, buffer_type,
        unit->frameNumber, unit->frameType, (jlong)unit->receiveTimeUs,
        (jlong)unit->enqueueTimeUs, (jlong)unit->presentationTimeUs);
    return clear_exception(env) ? DR_NEED_IDR : result;
}

static int video_submit(PDECODE_UNIT unit) {
    JNIEnv* env = get_env();
    if (env == NULL || g_receiver == NULL || g_decode_buffer == NULL ||
        unit == NULL || unit->fullLength <= 0 ||
        unit->fullLength > CAPYIO_MAX_DECODE_UNIT_BYTES) {
        return DR_NEED_IDR;
    }
    if ((*env)->GetArrayLength(env, g_decode_buffer) < unit->fullLength) {
        (*env)->DeleteGlobalRef(env, g_decode_buffer);
        jbyteArray local = (*env)->NewByteArray(env, unit->fullLength);
        if (local == NULL) {
            clear_exception(env);
            g_decode_buffer = NULL;
            return DR_NEED_IDR;
        }
        g_decode_buffer = (*env)->NewGlobalRef(env, local);
        (*env)->DeleteLocalRef(env, local);
        if (g_decode_buffer == NULL) {
            return DR_NEED_IDR;
        }
    }

    int picture_offset = 0;
    for (PLENTRY entry = unit->bufferList; entry != NULL; entry = entry->next) {
        if (entry->length <= 0 || entry->length > unit->fullLength) {
            return DR_NEED_IDR;
        }
        if (entry->bufferType == BUFFER_TYPE_PICDATA) {
            if (picture_offset > unit->fullLength - entry->length) {
                return DR_NEED_IDR;
            }
            (*env)->SetByteArrayRegion(env, g_decode_buffer, picture_offset,
                                       entry->length, (const jbyte*)entry->data);
            picture_offset += entry->length;
        } else {
            (*env)->SetByteArrayRegion(env, g_decode_buffer, 0, entry->length,
                                       (const jbyte*)entry->data);
            if (clear_exception(env)) {
                return DR_NEED_IDR;
            }
            int result = submit_one(env, entry->length, entry->bufferType, unit);
            if (result != DR_OK) {
                return result;
            }
        }
    }
    if (picture_offset == 0) {
        return DR_OK;
    }
    if (clear_exception(env)) {
        return DR_NEED_IDR;
    }
    return submit_one(env, picture_offset, BUFFER_TYPE_PICDATA, unit);
}

static int selected_video_format(int codec, int dynamic_range) {
    if (codec == 1 && dynamic_range == 1) return VIDEO_FORMAT_H264;
    if (codec == 2 && dynamic_range == 1) return VIDEO_FORMAT_H265;
    if (codec == 2 && dynamic_range == 2) return VIDEO_FORMAT_H265_MAIN10;
    if (codec == 3 && dynamic_range == 1) return VIDEO_FORMAT_AV1_MAIN8;
    if (codec == 3 && dynamic_range == 2) return VIDEO_FORMAT_AV1_MAIN10;
    return 0;
}

static int selected_server_codec(int codec, int dynamic_range) {
    if (codec == 1 && dynamic_range == 1) return SCM_H264;
    if (codec == 2 && dynamic_range == 1) return SCM_HEVC;
    if (codec == 2 && dynamic_range == 2) return SCM_HEVC_MAIN10;
    if (codec == 3 && dynamic_range == 1) return SCM_AV1_MAIN8;
    if (codec == 3 && dynamic_range == 2) return SCM_AV1_MAIN10;
    return 0;
}

JNIEXPORT jint JNICALL
Java_dev_capyio_display_app_GameStreamNativeClient_nativeRun(
    JNIEnv* env, jobject receiver, jstring host, jint port_base, jint width,
    jint height, jint refresh_millihertz, jint codec, jint dynamic_range,
    jint bitrate_kbps, jbyteArray key, jint key_id) {
    if (host == NULL || key == NULL || (*env)->GetArrayLength(env, key) != 16 ||
        port_base < 1024 || port_base > 65514 || width <= 0 || width > 8192 ||
        height <= 0 || height > 8192 || refresh_millihertz <= 0 ||
        refresh_millihertz > 500000 || bitrate_kbps <= 0 || bitrate_kbps > 200000) {
        return -1;
    }
    int video_format = selected_video_format(codec, dynamic_range);
    int server_codec = selected_server_codec(codec, dynamic_range);
    jobject owned_receiver = NULL;
    if (video_format == 0 || server_codec == 0) {
        return -1;
    }

    pthread_mutex_lock(&g_state_mutex);
    if (g_owned) {
        pthread_mutex_unlock(&g_state_mutex);
        return -2;
    }
    g_owned = true;
    g_stop_requested = false;
    g_terminated = false;
    g_termination_error = 0;
    g_receiver = (*env)->NewGlobalRef(env, receiver);
    pthread_mutex_unlock(&g_state_mutex);
    if (g_receiver == NULL) {
        pthread_mutex_lock(&g_state_mutex);
        g_owned = false;
        pthread_mutex_unlock(&g_state_mutex);
        return -1;
    }

    const char* host_chars = (*env)->GetStringUTFChars(env, host, NULL);
    if (host_chars == NULL) {
        clear_exception(env);
        goto Fail;
    }
    char rtsp_url[96];
    int written = snprintf(rtsp_url, sizeof(rtsp_url), "rtspenc://%s:%d",
                           host_chars, port_base + 21);
    if (written <= 0 || (size_t)written >= sizeof(rtsp_url)) {
        (*env)->ReleaseStringUTFChars(env, host, host_chars);
        goto Fail;
    }

    SERVER_INFORMATION server_info;
    STREAM_CONFIGURATION stream_config;
    CONNECTION_LISTENER_CALLBACKS connection_callbacks;
    DECODER_RENDERER_CALLBACKS video_callbacks;
    LiInitializeServerInformation(&server_info);
    LiInitializeStreamConfiguration(&stream_config);
    LiInitializeConnectionCallbacks(&connection_callbacks);
    LiInitializeVideoCallbacks(&video_callbacks);

    server_info.address = host_chars;
    server_info.serverInfoAppVersion = "7.1.431.-1";
    server_info.serverInfoGfeVersion = "3.23.0.74";
    server_info.rtspSessionUrl = rtsp_url;
    server_info.serverCodecModeSupport = server_codec;

    stream_config.width = width;
    stream_config.height = height;
    stream_config.fps = refresh_millihertz;
    stream_config.bitrate = bitrate_kbps;
    stream_config.packetSize = 1024;
    stream_config.streamingRemotely = STREAM_CFG_LOCAL;
    stream_config.audioConfiguration = AUDIO_CONFIGURATION_STEREO;
    stream_config.supportedVideoFormats = video_format;
    stream_config.clientRefreshRateX100 = (refresh_millihertz + 5) / 10;
    stream_config.colorSpace = dynamic_range == 2 ? COLORSPACE_REC_2020 : COLORSPACE_REC_709;
    stream_config.colorRange = COLOR_RANGE_LIMITED;
    stream_config.encryptionFlags = ENCFLG_ALL;

    jbyte key_bytes[16];
    (*env)->GetByteArrayRegion(env, key, 0, 16, key_bytes);
    if (clear_exception(env)) {
        (*env)->ReleaseStringUTFChars(env, host, host_chars);
        secure_zero(key_bytes, sizeof(key_bytes));
        goto Fail;
    }
    memcpy(stream_config.remoteInputAesKey, key_bytes, sizeof(key_bytes));
    secure_zero(key_bytes, sizeof(key_bytes));
    stream_config.remoteInputAesIv[0] = (char)((uint32_t)key_id >> 24);
    stream_config.remoteInputAesIv[1] = (char)((uint32_t)key_id >> 16);
    stream_config.remoteInputAesIv[2] = (char)((uint32_t)key_id >> 8);
    stream_config.remoteInputAesIv[3] = (char)((uint32_t)key_id);

    connection_callbacks.stageStarting = stage_starting;
    connection_callbacks.stageComplete = stage_complete;
    connection_callbacks.stageFailed = stage_failed;
    connection_callbacks.connectionStarted = connection_started;
    connection_callbacks.connectionTerminated = connection_terminated;
    connection_callbacks.logMessage = discard_log;
    video_callbacks.setup = video_setup;
    video_callbacks.start = video_start;
    video_callbacks.stop = video_stop;
    video_callbacks.cleanup = video_cleanup;
    video_callbacks.submitDecodeUnit = video_submit;
    video_callbacks.capabilities = 0;

    int result = LiStartConnection(&server_info, &stream_config,
                                   &connection_callbacks, &video_callbacks,
                                   NULL, NULL, 0, NULL, 0);
    secure_zero(stream_config.remoteInputAesKey, sizeof(stream_config.remoteInputAesKey));
    secure_zero(stream_config.remoteInputAesIv, sizeof(stream_config.remoteInputAesIv));
    (*env)->ReleaseStringUTFChars(env, host, host_chars);
    if (result == 0) {
        pthread_mutex_lock(&g_state_mutex);
        while (!g_stop_requested && !g_terminated) {
            pthread_cond_wait(&g_state_cond, &g_state_mutex);
        }
        int termination_error = g_termination_error;
        pthread_mutex_unlock(&g_state_mutex);
        LiStopConnection();
        result = termination_error;
    }

    pthread_mutex_lock(&g_state_mutex);
    owned_receiver = g_receiver;
    g_receiver = NULL;
    g_owned = false;
    pthread_mutex_unlock(&g_state_mutex);
    if (owned_receiver != NULL) {
        (*env)->DeleteGlobalRef(env, owned_receiver);
    }
    return result;

Fail:
    pthread_mutex_lock(&g_state_mutex);
    owned_receiver = g_receiver;
    g_receiver = NULL;
    g_owned = false;
    pthread_mutex_unlock(&g_state_mutex);
    if (owned_receiver != NULL) {
        (*env)->DeleteGlobalRef(env, owned_receiver);
    }
    return -1;
}

JNIEXPORT void JNICALL
Java_dev_capyio_display_app_GameStreamNativeClient_nativeInterrupt(
    JNIEnv* env, jobject receiver) {
    (void)env;
    (void)receiver;
    pthread_mutex_lock(&g_state_mutex);
    if (g_owned) {
        g_stop_requested = true;
        pthread_cond_signal(&g_state_cond);
    }
    pthread_mutex_unlock(&g_state_mutex);
    LiInterruptConnection();
}

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM* vm, void* reserved) {
    (void)reserved;
    g_vm = vm;
    JNIEnv* env = NULL;
    if ((*vm)->GetEnv(vm, (void**)&env, JNI_VERSION_1_6) != JNI_OK) {
        return JNI_ERR;
    }
    jclass local = (*env)->FindClass(env, "dev/capyio/display/app/GameStreamNativeClient");
    if (local == NULL) return JNI_ERR;
    g_on_stage = (*env)->GetMethodID(env, local, "onNativeStage", "(Ljava/lang/String;)V");
    g_on_connected = (*env)->GetMethodID(env, local, "onNativeConnected", "()V");
    g_on_terminated = (*env)->GetMethodID(env, local, "onNativeTerminated", "(I)V");
    g_on_video_setup = (*env)->GetMethodID(env, local, "onNativeVideoSetup", "(IIIII)I");
    g_on_video_start = (*env)->GetMethodID(env, local, "onNativeVideoStart", "()V");
    g_on_video_stop = (*env)->GetMethodID(env, local, "onNativeVideoStop", "()V");
    g_on_video_cleanup = (*env)->GetMethodID(env, local, "onNativeVideoCleanup", "()V");
    g_on_video_unit = (*env)->GetMethodID(env, local, "onNativeVideoUnit", "([BIIIIJJJ)I");
    if (clear_exception(env) || g_on_stage == NULL || g_on_connected == NULL ||
        g_on_terminated == NULL || g_on_video_setup == NULL ||
        g_on_video_start == NULL || g_on_video_stop == NULL ||
        g_on_video_cleanup == NULL || g_on_video_unit == NULL) {
        return JNI_ERR;
    }
    return JNI_VERSION_1_6;
}
