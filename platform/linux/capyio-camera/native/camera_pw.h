/* SPDX-License-Identifier: Apache-2.0 */
#ifndef CAPYIO_CAMERA_PW_H
#define CAPYIO_CAMERA_PW_H
#include "camera_protocol.h"
#include <pipewire/pipewire.h>
#include <spa/param/video/format-utils.h>
#include <spa/buffer/meta.h>
static inline struct spa_pod *camera_format(struct spa_pod_builder *b) {
    struct spa_video_info_raw f = {
        .format=SPA_VIDEO_FORMAT_NV12, .size={CAMERA_WIDTH,CAMERA_HEIGHT},
        .framerate={30,1}, .color_range=SPA_VIDEO_COLOR_RANGE_16_235,
        .color_matrix=SPA_VIDEO_COLOR_MATRIX_BT709,
        .transfer_function=SPA_VIDEO_TRANSFER_BT709,
        .color_primaries=SPA_VIDEO_COLOR_PRIMARIES_BT709,
        .interlace_mode=SPA_VIDEO_INTERLACE_MODE_PROGRESSIVE
    };
    return spa_format_video_raw_build(b,SPA_PARAM_EnumFormat,&f);
}
static inline bool camera_format_valid(const struct spa_pod *p) {
    struct spa_video_info_raw f={0};
    return spa_format_video_raw_parse(p,&f)>=0 && f.format==SPA_VIDEO_FORMAT_NV12 &&
        f.size.width==CAMERA_WIDTH && f.size.height==CAMERA_HEIGHT &&
        f.framerate.num==30 && f.framerate.denom==1 &&
        f.color_range==SPA_VIDEO_COLOR_RANGE_16_235 &&
        f.color_matrix==SPA_VIDEO_COLOR_MATRIX_BT709 &&
        f.transfer_function==SPA_VIDEO_TRANSFER_BT709 &&
        f.color_primaries==SPA_VIDEO_COLOR_PRIMARIES_BT709 &&
        f.interlace_mode==SPA_VIDEO_INTERLACE_MODE_PROGRESSIVE;
}
static inline int camera_buffers(struct pw_stream *stream) {
    uint8_t bytes[1024]; struct spa_pod_builder b=SPA_POD_BUILDER_INIT(bytes,sizeof(bytes));
    const struct spa_pod *p[2];
    p[0]=spa_pod_builder_add_object(&b,SPA_TYPE_OBJECT_ParamBuffers,SPA_PARAM_Buffers,
        SPA_PARAM_BUFFERS_buffers,SPA_POD_CHOICE_RANGE_Int(3,2,3),
        SPA_PARAM_BUFFERS_blocks,SPA_POD_Int(1),
        SPA_PARAM_BUFFERS_size,SPA_POD_Int(CAMERA_BYTES),
        SPA_PARAM_BUFFERS_stride,SPA_POD_Int(CAMERA_WIDTH),
        SPA_PARAM_BUFFERS_dataType,SPA_POD_CHOICE_FLAGS_Int((1<<SPA_DATA_MemPtr)|(1<<SPA_DATA_MemFd)));
    p[1]=spa_pod_builder_add_object(&b,SPA_TYPE_OBJECT_ParamMeta,SPA_PARAM_Meta,
        SPA_PARAM_META_type,SPA_POD_Id(SPA_META_Header),
        SPA_PARAM_META_size,SPA_POD_Int(sizeof(struct spa_meta_header)));
    return pw_stream_update_params(stream,p,2);
}
#endif
