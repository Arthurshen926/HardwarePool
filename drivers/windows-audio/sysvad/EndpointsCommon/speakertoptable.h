/*++

Copyright (c) Microsoft Corporation All Rights Reserved

Module Name:

    speakertoptable.h

Abstract:

    Declaration of topology tables.

--*/

#ifndef _SYSVAD_SPEAKERTOPTABLE_H_
#define _SYSVAD_SPEAKERTOPTABLE_H_

// CapyIO-owned endpoint connector identities. The Name GUID controls direct
// KSPROPERTY_PIN_NAME queries, while AudioEndpointBuilder initializes the
// endpoint DeviceDesc from the Category GUID. Keep both synchronized with
// ComponentizedAudioSample.inx.
// {c2ae0cd6-c228-41a6-8b0f-8b13773556a0}
DEFINE_GUID(CAPYIO_SPEAKER_CUSTOM_NAME,
0xc2ae0cd6, 0xc228, 0x41a6, 0x8b, 0x0f, 0x8b, 0x13, 0x77, 0x35, 0x56, 0xa0);

// {bec4e45e-4dd5-492b-91b0-596da93ccec5}
DEFINE_GUID(CAPYIO_MIC_INGRESS_CUSTOM_NAME,
0xbec4e45e, 0x4dd5, 0x492b, 0x91, 0xb0, 0x59, 0x6d, 0xa9, 0x3c, 0xce, 0xc5);

// {6f13d5db-0274-4e66-a116-340b4c54eb38}
DEFINE_GUID(CAPYIO_MIC_INGRESS_CATEGORY,
0x6f13d5db, 0x0274, 0x4e66, 0xa1, 0x16, 0x34, 0x0b, 0x4c, 0x54, 0xeb, 0x38);

//=============================================================================
static
KSDATARANGE SpeakerTopoPinDataRangesBridge[] =
{
 {
   sizeof(KSDATARANGE),
   0,
   0,
   0,
   STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
   STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
   STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)
 }
};

//=============================================================================
static
PKSDATARANGE SpeakerTopoPinDataRangePointersBridge[] =
{
  &SpeakerTopoPinDataRangesBridge[0]
};

//=============================================================================
static
PCPIN_DESCRIPTOR SpeakerTopoMiniportPins[] =
{
  // KSPIN_TOPO_WAVEOUT_SOURCE
  {
    0,
    0,
    0,                                                  // InstanceCount
    NULL,                                               // AutomationTable
    {                                                   // KsPinDescriptor
      0,                                                // InterfacesCount
      NULL,                                             // Interfaces
      0,                                                // MediumsCount
      NULL,                                             // Mediums
      SIZEOF_ARRAY(SpeakerTopoPinDataRangePointersBridge),// DataRangesCount
      SpeakerTopoPinDataRangePointersBridge,            // DataRanges
      KSPIN_DATAFLOW_IN,                                // DataFlow
      KSPIN_COMMUNICATION_NONE,                         // Communication
      &KSCATEGORY_AUDIO,                                // Category
      NULL,                                             // Name
      0                                                 // Reserved
    }
  },
  // KSPIN_TOPO_LINEOUT_DEST
  {
    0,
    0,
    0,                                                  // InstanceCount
    NULL,                                               // AutomationTable
    {                                                   // KsPinDescriptor
      0,                                                // InterfacesCount
      NULL,                                             // Interfaces
      0,                                                // MediumsCount
      NULL,                                             // Mediums
      SIZEOF_ARRAY(SpeakerTopoPinDataRangePointersBridge),// DataRangesCount
      SpeakerTopoPinDataRangePointersBridge,            // DataRanges
      KSPIN_DATAFLOW_OUT,                               // DataFlow
      KSPIN_COMMUNICATION_NONE,                         // Communication
      &KSNODETYPE_SPEAKER,                              // Category
      &CAPYIO_SPEAKER_CUSTOM_NAME,                      // Name
      0                                                 // Reserved
    }
  }
};

// The microphone ingress is also a render endpoint, but it must not inherit
// the speaker bridge-pin name. MicYou selects an output device by its exact
// MMDevice friendly name, so the two render endpoints require distinct pin
// names even though they share the same bounded WaveRT implementation.
static
PCPIN_DESCRIPTOR CapyIoMicrophoneIngressTopoMiniportPins[] =
{
  {
    0,
    0,
    0,
    NULL,
    {
      0,
      NULL,
      0,
      NULL,
      SIZEOF_ARRAY(SpeakerTopoPinDataRangePointersBridge),
      SpeakerTopoPinDataRangePointersBridge,
      KSPIN_DATAFLOW_IN,
      KSPIN_COMMUNICATION_NONE,
      &KSCATEGORY_AUDIO,
      NULL,
      0
    }
  },
  {
    0,
    0,
    0,
    NULL,
    {
      0,
      NULL,
      0,
      NULL,
      SIZEOF_ARRAY(SpeakerTopoPinDataRangePointersBridge),
      SpeakerTopoPinDataRangePointersBridge,
      KSPIN_DATAFLOW_OUT,
      KSPIN_COMMUNICATION_NONE,
      &CAPYIO_MIC_INGRESS_CATEGORY,
      &CAPYIO_MIC_INGRESS_CUSTOM_NAME,
      0
    }
  }
};

//=============================================================================
static
KSJACK_DESCRIPTION SpeakerJackDescBridge =
{
    KSAUDIO_SPEAKER_STEREO,
    0xB3C98C,               // Color spec for green
    eConnTypeUnknown,
    eGeoLocFront,
    eGenLocPrimaryBox,
    ePortConnIntegratedDevice,
    TRUE
};

// Only return a KSJACK_DESCRIPTION for the physical bridge pin.
static
PKSJACK_DESCRIPTION SpeakerJackDescriptions[] =
{
    NULL,
    &SpeakerJackDescBridge
};

static SYSVAD_AUDIOPOSTURE_INFO SpeakerAudioPostureInfo = { TRUE };

// Only support property for the physical bridge pin.
static
PSYSVAD_AUDIOPOSTURE_INFO SpeakerAudioPostureInfoPointers[]
{
    NULL,
    &SpeakerAudioPostureInfo
};

//=============================================================================
static
PCCONNECTION_DESCRIPTOR SpeakerTopoMiniportConnections[] =
{
  //  FromNode,                     FromPin,                        ToNode,                      ToPin
  {   PCFILTER_NODE,                KSPIN_TOPO_WAVEOUT_SOURCE,      PCFILTER_NODE,               KSPIN_TOPO_LINEOUT_DEST}
};


//=============================================================================
static
PCPROPERTY_ITEM PropertiesSpeakerTopoFilter[] =
{
    {
        &KSPROPSETID_Jack,
        KSPROPERTY_JACK_DESCRIPTION,
        KSPROPERTY_TYPE_GET |
        KSPROPERTY_TYPE_BASICSUPPORT,
        PropertyHandler_SpeakerTopoFilter
    },
    {
        &KSPROPSETID_Jack,
        KSPROPERTY_JACK_DESCRIPTION2,
        KSPROPERTY_TYPE_GET |
        KSPROPERTY_TYPE_BASICSUPPORT,
        PropertyHandler_SpeakerTopoFilter
    },
    {
        &KSPROPSETID_Jack,
        KSPROPERTY_JACK_DESCRIPTION3,
        KSPROPERTY_TYPE_GET |
        KSPROPERTY_TYPE_BASICSUPPORT,
        PropertyHandler_SpeakerTopoFilter
    },
    {
        &KSPROPSETID_AudioResourceManagement,
        KSPROPERTY_AUDIORESOURCEMANAGEMENT_RESOURCEGROUP,
        KSPROPERTY_TYPE_SET,
        PropertyHandler_SpeakerTopoFilter
    }
    ,{
        &KSPROPSETID_AudioPosture,
        KSPROPERTY_AUDIOPOSTURE_ORIENTATION,
        KSPROPERTY_TYPE_SET |
        KSPROPERTY_TYPE_BASICSUPPORT,
        PropertyHandler_SpeakerTopoFilter
    }
};

DEFINE_PCAUTOMATION_TABLE_PROP(AutomationSpeakerTopoFilter, PropertiesSpeakerTopoFilter);

//=============================================================================
static
PCFILTER_DESCRIPTOR SpeakerTopoMiniportFilterDescriptor =
{
  0,                                            // Version
  &AutomationSpeakerTopoFilter,                 // AutomationTable
  sizeof(PCPIN_DESCRIPTOR),                     // PinSize
  SIZEOF_ARRAY(SpeakerTopoMiniportPins),        // PinCount
  SpeakerTopoMiniportPins,                      // Pins
  sizeof(PCNODE_DESCRIPTOR),                    // NodeSize
  0,                                            // NodeCount
  NULL,                                         // Nodes
  SIZEOF_ARRAY(SpeakerTopoMiniportConnections), // ConnectionCount
  SpeakerTopoMiniportConnections,               // Connections
  0,                                            // CategoryCount
  NULL                                          // Categories
};

static
PCFILTER_DESCRIPTOR CapyIoMicrophoneIngressTopoMiniportFilterDescriptor =
{
  0,
  &AutomationSpeakerTopoFilter,
  sizeof(PCPIN_DESCRIPTOR),
  SIZEOF_ARRAY(CapyIoMicrophoneIngressTopoMiniportPins),
  CapyIoMicrophoneIngressTopoMiniportPins,
  sizeof(PCNODE_DESCRIPTOR),
  0,
  NULL,
  SIZEOF_ARRAY(SpeakerTopoMiniportConnections),
  SpeakerTopoMiniportConnections,
  0,
  NULL
};

#endif // _SYSVAD_SPEAKERTOPTABLE_H_
