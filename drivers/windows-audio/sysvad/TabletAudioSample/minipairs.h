/*++

Copyright (c) Microsoft Corporation All Rights Reserved

Module Name:

    minipairs.h

Abstract:

    CapyIO Speaker endpoint filter definitions, derived from SysVAD.

--*/

#ifndef _SYSVAD_MINIPAIRS_H_
#define _SYSVAD_MINIPAIRS_H_

#include "speakertopo.h"
#include "speakertoptable.h"
#include "speakerwavtable.h"

NTSTATUS
CreateMiniportWaveRTSYSVAD
(
    _Out_       PUNKNOWN *,
    _In_        REFCLSID,
    _In_opt_    PUNKNOWN,
    _In_        POOL_FLAGS,
    _In_        PUNKNOWN,
    _In_opt_    PVOID,
    _In_        PENDPOINT_MINIPAIR
);

NTSTATUS
CreateMiniportTopologySYSVAD
(
    _Out_       PUNKNOWN *,
    _In_        REFCLSID,
    _In_opt_    PUNKNOWN,
    _In_        POOL_FLAGS,
    _In_        PUNKNOWN,
    _In_opt_    PVOID,
    _In_        PENDPOINT_MINIPAIR
);

// Describe bounded WaveRT packet-size constraints for the render endpoint.
static struct
{
    KSAUDIO_PACKETSIZE_CONSTRAINTS2 TransportPacketConstraints;
    KSAUDIO_PACKETSIZE_PROCESSINGMODE_CONSTRAINT AdditionalProcessingConstraints[1];
} CapyIoWaveRtPacketSizeConstraintsRender =
{
    {
        2 * HNSTIME_PER_MILLISECOND,
        FILE_BYTE_ALIGNMENT,
        0,
        2,
        {
            STATIC_AUDIO_SIGNALPROCESSINGMODE_DEFAULT,
            128,
            0,
        },
    },
    {
        {
            STATIC_AUDIO_SIGNALPROCESSINGMODE_MOVIE,
            1024,
            0,
        },
    }
};

const SYSVAD_DEVPROPERTY CapyIoWaveFilterInterfacePropertiesRender[] =
{
    {
        &DEVPKEY_KsAudio_PacketSize_Constraints2,
        DEVPROP_TYPE_BINARY,
        sizeof(CapyIoWaveRtPacketSizeConstraintsRender),
        &CapyIoWaveRtPacketSizeConstraintsRender,
    },
};

static PHYSICALCONNECTIONTABLE CapyIoSpeakerTopologyPhysicalConnections[] =
{
    {
        KSPIN_TOPO_WAVEOUT_SOURCE,
        KSPIN_WAVE_RENDER_SOURCE,
        CONNECTIONTYPE_WAVE_OUTPUT
    }
};

static ENDPOINT_MINIPAIR CapyIoSpeakerMiniports =
{
    eSpeakerDevice,
    L"TopologySpeaker",
    NULL,
    CreateMiniportTopologySYSVAD,
    &SpeakerTopoMiniportFilterDescriptor,
    0, NULL,
    L"WaveSpeaker",
    NULL,
    CreateMiniportWaveRTSYSVAD,
    &SpeakerWaveMiniportFilterDescriptor,
    ARRAYSIZE(CapyIoWaveFilterInterfacePropertiesRender),
    CapyIoWaveFilterInterfacePropertiesRender,
    SPEAKER_DEVICE_MAX_CHANNELS,
    SpeakerPinDeviceFormatsAndModes,
    SIZEOF_ARRAY(SpeakerPinDeviceFormatsAndModes),
    CapyIoSpeakerTopologyPhysicalConnections,
    SIZEOF_ARRAY(CapyIoSpeakerTopologyPhysicalConnections),
    ENDPOINT_OFFLOAD_SUPPORTED,
    SpeakerModulesWaveFilter,
    SIZEOF_ARRAY(SpeakerModulesWaveFilter),
    &SpeakerModuleNotificationDeviceId,
};

static PENDPOINT_MINIPAIR g_RenderEndpoints[] =
{
    &CapyIoSpeakerMiniports,
};

#define g_cRenderEndpoints (SIZEOF_ARRAY(g_RenderEndpoints))

static PENDPOINT_MINIPAIR* g_CaptureEndpoints = nullptr;
static ULONG g_cCaptureEndpoints = 0;

#define g_MaxMiniports (g_cRenderEndpoints * 2)

#endif // _SYSVAD_MINIPAIRS_H_
