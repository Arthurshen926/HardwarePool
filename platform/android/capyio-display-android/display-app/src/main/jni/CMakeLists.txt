cmake_minimum_required(VERSION 3.22)
project(capyio_gamestream_client C)

if(NOT DEFINED CAPYIO_MOONLIGHT_ROOT)
  message(FATAL_ERROR "CAPYIO_MOONLIGHT_ROOT must name the hash-verified staged source directory")
endif()

set(COMMON "${CAPYIO_MOONLIGHT_ROOT}/moonlight-common-c")
set(OPENSSL_ROOT "${CAPYIO_MOONLIGHT_ROOT}/openssl")
set(COMMON_SOURCES
  "${COMMON}/src/ByteBuffer.c"
  "${COMMON}/src/Connection.c"
  "${COMMON}/src/ControlStream.c"
  "${COMMON}/src/FakeCallbacks.c"
  "${COMMON}/src/LinkedBlockingQueue.c"
  "${COMMON}/src/Misc.c"
  "${COMMON}/src/Platform.c"
  "${COMMON}/src/PlatformCrypto.c"
  "${COMMON}/src/PlatformSockets.c"
  "${COMMON}/src/RtpVideoQueue.c"
  "${COMMON}/src/RtspConnection.c"
  "${COMMON}/src/RtspParser.c"
  "${COMMON}/src/SdpGenerator.c"
  "${COMMON}/src/SimpleStun.c"
  "${COMMON}/src/VideoDepacketizer.c"
  "${COMMON}/src/VideoStream.c"
  "${COMMON}/nanors/deps/obl/oblas_common.c"
  "${COMMON}/nanors/deps/obl/oblas_lite.c"
  "${COMMON}/nanors/rs.c"
  "${COMMON}/enet/callbacks.c"
  "${COMMON}/enet/compress.c"
  "${COMMON}/enet/host.c"
  "${COMMON}/enet/list.c"
  "${COMMON}/enet/packet.c"
  "${COMMON}/enet/peer.c"
  "${COMMON}/enet/protocol.c"
  "${COMMON}/enet/unix.c"
)

foreach(FORBIDDEN AudioStream.c RtpAudioQueue.c InputStream.c)
  list(FIND COMMON_SOURCES "${COMMON}/src/${FORBIDDEN}" FORBIDDEN_INDEX)
  if(NOT FORBIDDEN_INDEX EQUAL -1)
    message(FATAL_ERROR "display-only target includes forbidden source ${FORBIDDEN}")
  endif()
endforeach()

add_library(capyio_crypto STATIC IMPORTED)
set_target_properties(capyio_crypto PROPERTIES
  IMPORTED_LOCATION "${OPENSSL_ROOT}/${ANDROID_ABI}/libcrypto.a"
  INTERFACE_INCLUDE_DIRECTORIES "${OPENSSL_ROOT}/include"
)

add_library(capyio-gamestream-client SHARED
  capyio_gamestream_client.c
  ${COMMON_SOURCES}
)
target_compile_features(capyio-gamestream-client PRIVATE c_std_11)
target_compile_definitions(capyio-gamestream-client PRIVATE
  HAS_SOCKLEN_T=1
  LC_ANDROID
  HAVE_CLOCK_GETTIME=1
  CAPYIO_VIDEO_ONLY_CLIENT
)
target_compile_options(capyio-gamestream-client PRIVATE
  -Wall -Wextra -Werror -Wno-unused-parameter
)
target_include_directories(capyio-gamestream-client PRIVATE
  "${COMMON}/src"
  "${COMMON}/enet/include"
  "${COMMON}/nanors"
  "${COMMON}/nanors/deps"
  "${COMMON}/nanors/deps/obl"
)
find_library(LOG_LIBRARY log REQUIRED)
target_link_libraries(capyio-gamestream-client PRIVATE
  capyio_crypto
  ${LOG_LIBRARY}
)
# OpenSSL's Android archive contains optimized assembly with local PC-relative
# relocations. Keep every archive symbol private to this JNI boundary, matching
# Moonlight Android's upstream link contract and avoiding dynamic export
# relocations for those implementation details.
target_link_options(capyio-gamestream-client PRIVATE
  "LINKER:--exclude-libs,ALL"
)
