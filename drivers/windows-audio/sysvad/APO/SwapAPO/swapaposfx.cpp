//
// SwapAPOSFX.cpp -- Copyright (c) Microsoft Corporation. All rights reserved.
//
// Description:
//
//  Implementation of CSwapAPOSFX
//

#include <atlbase.h>
#include <atlcom.h>
#include <atlcoll.h>
#include <atlsync.h>
#include <mmreg.h>

#include <audioenginebaseapo.h>
#include <baseaudioprocessingobject.h>
#include <endpointvolume.h>
#include <resource.h>

#include <float.h>

#include "SwapAPO.h"
#include <devicetopology.h>
#include <CustomPropKeys.h>
#include <propvarutil.h>

namespace
{
const PROPERTYKEY kAudioEndpointAssociation =
{
    {0x1da5d803, 0xd492, 0x4edd, {0x8c, 0x23, 0xe0, 0xc0, 0xff, 0xee, 0x7f, 0x0e}},
    2
};
const GUID kCapyIoMicrophoneIngressCategory =
    {0x6f13d5db, 0x0274, 0x4e66, {0xa1, 0x16, 0x34, 0x0b, 0x4c, 0x54, 0xeb, 0x38}};
const GUID kMicrophoneCategory =
    {0xdff21be1, 0xf70f, 0x11d0, {0xb9, 0x17, 0x00, 0xa0, 0xc9, 0x22, 0x31, 0x96}};

MicrophoneBridgeRole GetMicrophoneBridgeRole(IMMDevice* endpoint)
{
    if (endpoint == nullptr)
    {
        return MicrophoneBridgeRole::Detached;
    }

    wil::com_ptr_nothrow<IPropertyStore> store;
    if (FAILED(endpoint->OpenPropertyStore(STGM_READ, store.put())))
    {
        return MicrophoneBridgeRole::Detached;
    }

    PROPVARIANT value;
    PropVariantInit(&value);
    const HRESULT hr = store->GetValue(kAudioEndpointAssociation, &value);
    MicrophoneBridgeRole role = MicrophoneBridgeRole::Detached;
    GUID association = GUID_NULL;
    bool hasAssociation = false;
    if (SUCCEEDED(hr) && value.vt == VT_CLSID && value.puuid != nullptr)
    {
        association = *value.puuid;
        hasAssociation = true;
    }
    else if (SUCCEEDED(hr) && value.vt == VT_LPWSTR && value.pwszVal != nullptr)
    {
        hasAssociation = SUCCEEDED(CLSIDFromString(value.pwszVal, &association));
    }
    if (hasAssociation)
    {
        if (IsEqualGUID(association, kCapyIoMicrophoneIngressCategory))
        {
            role = MicrophoneBridgeRole::IngressProducer;
        }
        else if (IsEqualGUID(association, kMicrophoneCategory))
        {
            role = MicrophoneBridgeRole::CaptureConsumer;
        }
    }
    PropVariantClear(&value);
    return role;
}
}


// Static declaration of the APO_REG_PROPERTIES structure
// associated with this APO.  The number in <> brackets is the
// number of IIDs supported by this APO.  If more than one, then additional
// IIDs are added at the end
#pragma warning (disable : 4815)
const AVRT_DATA CRegAPOProperties<1> CSwapAPOSFX::sm_RegProperties(
    __uuidof(SwapAPOSFX),                           // clsid of this APO
    L"CSwapAPOSFX",                                 // friendly name of this APO
    L"Copyright (c) Microsoft Corporation",         // copyright info
    1,                                              // major version #
    0,                                              // minor version #
    __uuidof(ISwapAPOSFX)                           // iid of primary interface

//
// If you need to change any of these attributes, uncomment everything up to
// the point that you need to change something.  If you need to add IIDs, uncomment
// everything and add additional IIDs at the end.
//
//  , DEFAULT_APOREG_FLAGS
//  , DEFAULT_APOREG_MININPUTCONNECTIONS
//  , DEFAULT_APOREG_MAXINPUTCONNECTIONS
//  , DEFAULT_APOREG_MINOUTPUTCONNECTIONS
//  , DEFAULT_APOREG_MAXOUTPUTCONNECTIONS
//  , DEFAULT_APOREG_MAXINSTANCES
//
    );

#pragma AVRT_CODE_BEGIN
//-------------------------------------------------------------------------
// Description:
//
//  Do the actual processing of data.
//
// Parameters:
//
//      u32NumInputConnections      - [in] number of input connections
//      ppInputConnections          - [in] pointer to list of input APO_CONNECTION_PROPERTY pointers
//      u32NumOutputConnections      - [in] number of output connections
//      ppOutputConnections         - [in] pointer to list of output APO_CONNECTION_PROPERTY pointers
//
// Return values:
//
//      void
//
// Remarks:
//
//  This function processes data in a manner dependent on the implementing
//  object.  This routine can not fail and can not block, or call any other
//  routine that blocks, or touch pagable memory.
//
STDMETHODIMP_(void) CSwapAPOSFX::APOProcess(
    UINT32 u32NumInputConnections,
    APO_CONNECTION_PROPERTY** ppInputConnections,
    UINT32 u32NumOutputConnections,
    APO_CONNECTION_PROPERTY** ppOutputConnections)
{
    UNREFERENCED_PARAMETER(u32NumInputConnections);
    UNREFERENCED_PARAMETER(u32NumOutputConnections);

    FLOAT32 *pf32InputFrames, *pf32OutputFrames;

    ATLASSERT(m_bIsLocked);

    // assert that the number of input and output connectins fits our registration properties
    ATLASSERT(m_pRegProperties->u32MinInputConnections <= u32NumInputConnections);
    ATLASSERT(m_pRegProperties->u32MaxInputConnections >= u32NumInputConnections);
    ATLASSERT(m_pRegProperties->u32MinOutputConnections <= u32NumOutputConnections);
    ATLASSERT(m_pRegProperties->u32MaxOutputConnections >= u32NumOutputConnections);

    // check APO_BUFFER_FLAGS.
    switch( ppInputConnections[0]->u32BufferFlags )
    {
        case BUFFER_INVALID:
        {
            ATLASSERT(false);  // invalid flag - should never occur.  don't do anything.
            break;
        }
        case BUFFER_VALID:
        case BUFFER_SILENT:
        {
            // get input pointer to connection buffer
            pf32InputFrames = reinterpret_cast<FLOAT32*>(ppInputConnections[0]->pBuffer);
            ATLASSERT( IS_VALID_TYPED_READ_POINTER(pf32InputFrames) );

            // get output pointer to connection buffer
            pf32OutputFrames = reinterpret_cast<FLOAT32*>(ppOutputConnections[0]->pBuffer);
            ATLASSERT( IS_VALID_TYPED_WRITE_POINTER(pf32OutputFrames) );

            if (BUFFER_SILENT == ppInputConnections[0]->u32BufferFlags)
            {
                WriteSilence( pf32InputFrames,
                              ppInputConnections[0]->u32ValidFrameCount,
                              GetSamplesPerFrame() );
            }

            const UINT32 frameCount = ppInputConnections[0]->u32ValidFrameCount;
            if (m_microphoneBridgeRole == MicrophoneBridgeRole::CaptureConsumer)
            {
                const std::uint32_t copied = m_captureConsumer.TryRead(
                    pf32OutputFrames,
                    frameCount);
                ppOutputConnections[0]->u32BufferFlags =
                    copied == 0 ? BUFFER_SILENT : BUFFER_VALID;
                ppOutputConnections[0]->u32ValidFrameCount = frameCount;
                break;
            }

            if (m_microphoneBridgeRole == MicrophoneBridgeRole::IngressProducer &&
                ppInputConnections[0]->u32BufferFlags == BUFFER_VALID)
            {
                m_captureProducer.TryWrite(
                    pf32InputFrames,
                    frameCount,
                    GetSamplesPerFrame());
            }

            // The producer performs one bounded copy into pre-mapped shared
            // memory. Ring absence/full/oversize degrades to a silent drop and
            // never blocks the Windows audio-engine callback.
            if (m_microphoneBridgeRole == MicrophoneBridgeRole::Detached)
            {
                m_renderRing.TryWrite(
                    pf32InputFrames,
                    frameCount,
                    GetSamplesPerFrame(),
                    InterlockedCompareExchange(&m_endpointGainMillion, 0, 0));
            }

            // swap the input buffer in-place
            if (
                m_microphoneBridgeRole == MicrophoneBridgeRole::Detached &&
                !IsEqualGUID(m_AudioProcessingMode, AUDIO_SIGNALPROCESSINGMODE_RAW) &&
                m_fEnableSwapSFX
            )
            {
                ProcessSwap(pf32InputFrames, pf32InputFrames,
                            frameCount,
                            m_u32SamplesPerFrame);
            }

            // copy the memory only if there is an output connection, and input/output pointers are unequal
            if ( (0 != u32NumOutputConnections) &&
                  (ppOutputConnections[0]->pBuffer != ppInputConnections[0]->pBuffer) )
            {
                CopyFrames( pf32OutputFrames, pf32InputFrames,
                            frameCount,
                            GetSamplesPerFrame() );
            }

            // pass along buffer flags
            ppOutputConnections[0]->u32BufferFlags = ppInputConnections[0]->u32BufferFlags;

            // Set the valid frame count.
            ppOutputConnections[0]->u32ValidFrameCount = frameCount;

            break;
        }
        default:
        {
            ATLASSERT(false);  // invalid flag - should never occur
            break;
        }
    } // switch

} // APOProcess
#pragma AVRT_CODE_END

//-------------------------------------------------------------------------
// Description:
//
//  Report delay added by the APO between samples given on input
//  and samples given on output.
//
// Parameters:
//
//      pTime                       - [out] hundreds-of-nanoseconds of delay added
//
// Return values:
//
//      S_OK on success, a failure code on failure
STDMETHODIMP CSwapAPOSFX::GetLatency(HNSTIME* pTime)
{
    ASSERT_NONREALTIME();
    HRESULT hr = S_OK;

    IF_TRUE_ACTION_JUMP(NULL == pTime, hr = E_POINTER, Exit);

    *pTime = 0;

Exit:
    return hr;
}

//-------------------------------------------------------------------------
// Description:
//
//  Verifies that the APO is ready to process and locks its state if so.
//
// Parameters:
//
//      u32NumInputConnections - [in] number of input connections attached to this APO
//      ppInputConnections - [in] connection descriptor of each input connection attached to this APO
//      u32NumOutputConnections - [in] number of output connections attached to this APO
//      ppOutputConnections - [in] connection descriptor of each output connection attached to this APO
//
// Return values:
//
//      S_OK                                Object is locked and ready to process.
//      E_POINTER                           Invalid pointer passed to function.
//      APOERR_INVALID_CONNECTION_FORMAT    Invalid connection format.
//      APOERR_NUM_CONNECTIONS_INVALID      Number of input or output connections is not valid on
//                                          this APO.
STDMETHODIMP CSwapAPOSFX::LockForProcess(UINT32 u32NumInputConnections,
    APO_CONNECTION_DESCRIPTOR** ppInputConnections,
    UINT32 u32NumOutputConnections, APO_CONNECTION_DESCRIPTOR** ppOutputConnections)
{
    ASSERT_NONREALTIME();
    capyio::capture_ring::RecordDiagnostic(500, S_OK);
    HRESULT hr = S_OK;
    UNCOMPRESSEDAUDIOFORMAT inputFormat = {};

    // The capture consumer replaces the synthetic SysVAD input with frames
    // from the shared microphone ring. CBaseAudioProcessingObject assumes an
    // in-place DSP and rejects graphs whose driver-side and engine-side sample
    // containers differ, even though this APO never consumes the former. Keep
    // this exception restricted to the fixed mono 48 kHz capture contract.
    if (m_microphoneBridgeRole == MicrophoneBridgeRole::Detached &&
        u32NumOutputConnections == 1 &&
        ppOutputConnections != nullptr &&
        ppOutputConnections[0] != nullptr &&
        ppOutputConnections[0]->pFormat != nullptr)
    {
        UNCOMPRESSEDAUDIOFORMAT candidateOutput = {};
        if (SUCCEEDED(ppOutputConnections[0]->pFormat->GetUncompressedAudioFormat(
                &candidateOutput)) &&
            candidateOutput.dwSamplesPerFrame == capyio::capture_ring::kChannels &&
            static_cast<std::uint32_t>(candidateOutput.fFramesPerSecond) ==
                capyio::capture_ring::kSampleRate)
        {
            m_microphoneBridgeRole = MicrophoneBridgeRole::CaptureConsumer;
        }
    }
    if (m_microphoneBridgeRole == MicrophoneBridgeRole::CaptureConsumer)
    {
        UNCOMPRESSEDAUDIOFORMAT outputFormat = {};
        RETURN_HR_IF(E_INVALIDARG,
                     m_bIsLocked ||
                     u32NumInputConnections != 1 ||
                     u32NumOutputConnections != 1 ||
                     ppInputConnections == nullptr ||
                     ppOutputConnections == nullptr ||
                     ppInputConnections[0] == nullptr ||
                     ppOutputConnections[0] == nullptr ||
                     ppOutputConnections[0]->pFormat == nullptr);
        RETURN_IF_FAILED(
            ppOutputConnections[0]->pFormat->GetUncompressedAudioFormat(&outputFormat));
        RETURN_HR_IF(APOERR_INVALID_CONNECTION_FORMAT,
                     outputFormat.dwSamplesPerFrame != capyio::capture_ring::kChannels ||
                     static_cast<std::uint32_t>(outputFormat.fFramesPerSecond) !=
                         capyio::capture_ring::kSampleRate);

        m_u32SamplesPerFrame = outputFormat.dwSamplesPerFrame;
        m_bIsLocked = true;
        m_captureConsumer.Attach(
            static_cast<std::uint32_t>(outputFormat.fFramesPerSecond),
            outputFormat.dwSamplesPerFrame);
        return S_OK;
    }

    hr = CBaseAudioProcessingObject::LockForProcess(u32NumInputConnections,
        ppInputConnections, u32NumOutputConnections, ppOutputConnections);
    IF_FAILED_JUMP(hr, Exit);

    // Keep the callback lifetime bounded to the streaming lock. Registering
    // adds a COM reference to this APO, so unregistering in the destructor
    // would create the reference cycle documented by EndpointVolume.
    if (m_microphoneBridgeRole == MicrophoneBridgeRole::Detached &&
        m_endpointVolume != nullptr && !m_bRegisteredEndpointVolumeCallback)
    {
        if (SUCCEEDED(m_endpointVolume->RegisterControlChangeNotify(this)))
        {
            m_bRegisteredEndpointVolumeCallback = TRUE;

            // Close the registration/query race and refresh state for every
            // new processing lock.
            BOOL muted = FALSE;
            float scalar = 1.0f;
            if (SUCCEEDED(m_endpointVolume->GetMute(&muted)) &&
                SUCCEEDED(m_endpointVolume->GetMasterVolumeLevelScalar(&scalar)) &&
                scalar >= 0.0f && scalar <= 1.0f)
            {
                const LONG gain = muted
                    ? 0
                    : static_cast<LONG>(scalar * capyio::render_ring::kUnityGainMillion + 0.5f);
                InterlockedExchange(&m_endpointGainMillion, gain);
            }
        }
    }

    // The Broker must create and initialize the mapping before playback. This
    // open/validation work is deliberately outside APOProcess.
    if (SUCCEEDED(ppInputConnections[0]->pFormat->GetUncompressedAudioFormat(&inputFormat)))
    {
        if (m_microphoneBridgeRole == MicrophoneBridgeRole::CaptureConsumer)
        {
            m_captureConsumer.Attach(
                static_cast<std::uint32_t>(inputFormat.fFramesPerSecond),
                inputFormat.dwSamplesPerFrame);
        }
        else if (m_microphoneBridgeRole == MicrophoneBridgeRole::IngressProducer)
        {
            m_captureProducer.Attach(
                static_cast<std::uint32_t>(inputFormat.fFramesPerSecond),
                inputFormat.dwSamplesPerFrame);
        }
        else
        {
            m_renderRing.Attach(
                static_cast<std::uint32_t>(inputFormat.fFramesPerSecond),
                inputFormat.dwSamplesPerFrame);
        }
    }

Exit:
    return hr;
}

STDMETHODIMP CSwapAPOSFX::UnlockForProcess()
{
    ASSERT_NONREALTIME();
    if (m_microphoneBridgeRole == MicrophoneBridgeRole::CaptureConsumer)
    {
        m_captureConsumer.Detach();
        m_bIsLocked = false;
        m_u32SamplesPerFrame = 0;
        return S_OK;
    }
    if (m_bRegisteredEndpointVolumeCallback && m_endpointVolume != nullptr)
    {
        m_endpointVolume->UnregisterControlChangeNotify(this);
        m_bRegisteredEndpointVolumeCallback = FALSE;
    }
    m_renderRing.Detach();
    m_captureProducer.Detach();
    m_captureConsumer.Detach();
    return CBaseAudioProcessingObject::UnlockForProcess();
}

STDMETHODIMP CSwapAPOSFX::IsOutputFormatSupported(
    IAudioMediaType* pInputFormat,
    IAudioMediaType* pRequestedOutputFormat,
    IAudioMediaType** ppSupportedOutputFormat)
{
    capyio::capture_ring::RecordDiagnostic(400, S_OK);
    RETURN_HR_IF(E_POINTER,
                 pRequestedOutputFormat == nullptr ||
                 ppSupportedOutputFormat == nullptr);
    *ppSupportedOutputFormat = nullptr;

    // A capture MFX sits between the driver's fixed 16-bit PCM device format
    // and the Audio Engine's 32-bit float mix format. The base APO helper only
    // accepts an in-place format pair and therefore rejects this valid graph
    // before LockForProcess. CapyIO replaces the synthetic driver frames, so
    // accept only the project's fixed mono 48 kHz engine-side contract here.
    UNCOMPRESSEDAUDIOFORMAT outputFormat = {};
    if ((m_microphoneBridgeRole == MicrophoneBridgeRole::CaptureConsumer ||
         m_microphoneBridgeRole == MicrophoneBridgeRole::Detached) &&
        SUCCEEDED(pRequestedOutputFormat->GetUncompressedAudioFormat(&outputFormat)) &&
        outputFormat.dwSamplesPerFrame == capyio::capture_ring::kChannels &&
        static_cast<std::uint32_t>(outputFormat.fFramesPerSecond) ==
            capyio::capture_ring::kSampleRate)
    {
        *ppSupportedOutputFormat = pRequestedOutputFormat;
        (*ppSupportedOutputFormat)->AddRef();
        capyio::capture_ring::RecordDiagnostic(401, S_OK);
        return S_OK;
    }

    const HRESULT hr = CBaseAudioProcessingObject::IsOutputFormatSupported(
        pInputFormat,
        pRequestedOutputFormat,
        ppSupportedOutputFormat);
    capyio::capture_ring::RecordDiagnostic(402, hr);
    return hr;
}

// The method that this long comment refers to is "Initialize()"
//-------------------------------------------------------------------------
// Description:
//
//  Generic initialization routine for APOs.
//
// Parameters:
//
//     cbDataSize - [in] the size in bytes of the initialization data.
//     pbyData - [in] initialization data specific to this APO
//
// Return values:
//
//     S_OK                         Successful completion.
//     E_POINTER                    Invalid pointer passed to this function.
//     E_INVALIDARG                 Invalid argument
//     AEERR_ALREADY_INITIALIZED    APO is already initialized
//
// Remarks:
//
//  This method initializes the APO.  The data is variable length and
//  should have the form of:
//
//    struct MyAPOInitializationData
//    {
//        APOInitBaseStruct APOInit;
//        ... // add additional fields here
//    };
//
//  If the APO needs no initialization or needs no data to initialize
//  itself, it is valid to pass NULL as the pbyData parameter and 0 as
//  the cbDataSize parameter.
//
//  As part of designing an APO, decide which parameters should be
//  immutable (set once during initialization) and which mutable (changeable
//  during the lifetime of the APO instance).  Immutable parameters must
//  only be specifiable in the Initialize call; mutable parameters must be
//  settable via methods on whichever parameter control interface(s) your
//  APO provides. Mutable values should either be set in the initialize
//  method (if they are required for proper operation of the APO prior to
//  LockForProcess) or default to reasonable values upon initialize and not
//  be required to be set before LockForProcess.
//
//  Within the mutable parameters, you must also decide which can be changed
//  while the APO is locked for processing and which cannot.
//
//  All parameters should be considered immutable as a first choice, unless
//  there is a specific scenario which requires them to be mutable; similarly,
//  no mutable parameters should be changeable while the APO is locked, unless
//  a specific scenario requires them to be.  Following this guideline will
//  simplify the APO's state diagram and implementation and prevent certain
//  types of bug.
//
//  If a parameter changes the APOs latency or MaxXXXFrames values, it must be
//  immutable.
//
//  The default version of this function uses no initialization data, but does verify
//  the passed parameters and set the m_bIsInitialized member to true.
//
//  Note: This method may not be called from a real-time processing thread.
//

HRESULT CSwapAPOSFX::Initialize(UINT32 cbDataSize, BYTE* pbyData)
{
    capyio::capture_ring::RecordDiagnostic(300, static_cast<LONG>(cbDataSize));
    GUID processingMode = GUID_NULL;
    IMMDeviceCollection* deviceCollection = nullptr;

    RETURN_HR_IF(E_INVALIDARG, pbyData == nullptr || cbDataSize == 0);

    if (cbDataSize == sizeof(APOInitSystemEffects3))
    {
        auto* init = reinterpret_cast<APOInitSystemEffects3*>(pbyData);
        processingMode = init->AudioProcessingMode;
        deviceCollection = init->pDeviceCollection;
    }
    else if (cbDataSize == sizeof(APOInitSystemEffects2))
    {
        auto* init = reinterpret_cast<APOInitSystemEffects2*>(pbyData);
        processingMode = init->AudioProcessingMode;
        deviceCollection = init->pDeviceCollection;
    }
    else if (cbDataSize == sizeof(APOInitSystemEffects))
    {
        processingMode = AUDIO_SIGNALPROCESSINGMODE_DEFAULT;
    }
    else
    {
        capyio::capture_ring::RecordDiagnostic(301, E_INVALIDARG);
        return E_INVALIDARG;
    }

    // Validate then save the processing mode. The bridge is registered as a
    // post-mix mode effect; retain GUID_NULL for compatibility with older
    // system-effects initialization data.
    if (processingMode != GUID_NULL                                 &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_DEFAULT        &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_RAW            &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_COMMUNICATIONS &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_SPEECH         &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_MEDIA          &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_MOVIE          &&
        processingMode != AUDIO_SIGNALPROCESSINGMODE_NOTIFICATION)
    {
        capyio::capture_ring::RecordDiagnostic(302, E_INVALIDARG);
        return E_INVALIDARG;
    }
    m_AudioProcessingMode = processingMode;

    // SystemEffects2/3 collections may contain both topology and endpoint
    // devices. Find the item that actually exposes the endpoint Association;
    // retain the final item only as the speaker-volume fallback.
    if (deviceCollection != nullptr)
    {
        UINT32 deviceCount = 0;
        if (SUCCEEDED(deviceCollection->GetCount(&deviceCount)) && deviceCount > 0)
        {
            for (UINT32 index = 0; index < deviceCount; ++index)
            {
                wil::com_ptr_nothrow<IMMDevice> candidate;
                if (FAILED(deviceCollection->Item(index, candidate.put())))
                {
                    continue;
                }
                const MicrophoneBridgeRole candidateRole =
                    GetMicrophoneBridgeRole(candidate.get());
                if (candidateRole != MicrophoneBridgeRole::Detached)
                {
                    m_audioEndpoint = candidate;
                    m_microphoneBridgeRole = candidateRole;
                    break;
                }
                if (index == deviceCount - 1)
                {
                    m_audioEndpoint = candidate;
                }
            }
            if (m_microphoneBridgeRole == MicrophoneBridgeRole::Detached &&
                m_audioEndpoint != nullptr &&
                SUCCEEDED(m_audioEndpoint->Activate(
                    __uuidof(IAudioEndpointVolume),
                    CLSCTX_ALL,
                    nullptr,
                    m_endpointVolume.put_void())))
            {
                BOOL muted = FALSE;
                float scalar = 1.0f;
                if (SUCCEEDED(m_endpointVolume->GetMute(&muted)) &&
                    SUCCEEDED(m_endpointVolume->GetMasterVolumeLevelScalar(&scalar)) &&
                    scalar >= 0.0f && scalar <= 1.0f)
                {
                    const LONG gain = muted
                        ? 0
                        : static_cast<LONG>(scalar * capyio::render_ring::kUnityGainMillion + 0.5f);
                    InterlockedExchange(&m_endpointGainMillion, gain);
                }
            }
        }
    }

    // CapyIO uses this post-mix effect only as a bounded render-ring bridge.
    // It must remain active, while the inherited SysVAD channel-swap DSP stays off.
    m_fEnableSwapSFX = FALSE;
    RtlZeroMemory(m_effectInfos, sizeof(m_effectInfos));
    m_effectInfos[0] = { SwapEffectId, FALSE, AUDIO_SYSTEMEFFECT_STATE_ON };

    m_bIsInitialized = true;
    capyio::capture_ring::RecordDiagnostic(
        305,
        static_cast<LONG>(m_microphoneBridgeRole));
    return S_OK;
}

HRESULT CSwapAPOSFX::OnNotify(PAUDIO_VOLUME_NOTIFICATION_DATA notificationData)
{
    RETURN_HR_IF(E_POINTER, notificationData == nullptr);

    const float scalar = notificationData->fMasterVolume;
    RETURN_HR_IF(E_INVALIDARG, !(scalar >= 0.0f && scalar <= 1.0f));

    const LONG gain = notificationData->bMuted
        ? 0
        : static_cast<LONG>(scalar * capyio::render_ring::kUnityGainMillion + 0.5f);
    InterlockedExchange(&m_endpointGainMillion, gain);
    return S_OK;
}

//-------------------------------------------------------------------------
//
// GetEffectsList
//
//  Retrieves the list of signal processing effects currently active and
//  stores an event to be signaled if the list changes.
//
// Parameters
//
//  ppEffectsIds - returns a pointer to a list of GUIDs each identifying a
//      class of effect. The caller is responsible for freeing this memory by
//      calling CoTaskMemFree.
//
//  pcEffects - returns a count of GUIDs in the list.
//
//  Event - passes an event handle. The APO signals this event when the list
//      of effects changes from the list returned from this function. The APO
//      uses this event until either this function is called again or the APO
//      is destroyed. The passed handle may be NULL. In this case, the APO
//      stops using any previous handle and does not signal an event.
//
// Remarks
//
//  An APO imlements this method to allow Windows to discover the current
//  effects applied by the APO. The list of effects may depend on what signal
//  processing mode the APO initialized (see AudioProcessingMode in the
//  APOInitSystemEffects2 structure) as well as any end user configuration.
//
//  If there are no effects then the function still succeeds, ppEffectsIds
//  returns a NULL pointer, and pcEffects returns a count of 0.
//
STDMETHODIMP CSwapAPOSFX::GetEffectsList(_Outptr_result_buffer_maybenull_(*pcEffects) LPGUID *ppEffectsIds, _Out_ UINT *pcEffects, _In_ HANDLE Event)
{
    HRESULT hr;
    BOOL effectsLocked = FALSE;
    UINT cEffects = 0;

    IF_TRUE_ACTION_JUMP(ppEffectsIds == NULL, hr = E_POINTER, Exit);
    IF_TRUE_ACTION_JUMP(pcEffects == NULL, hr = E_POINTER, Exit);

    // Synchronize access to the effects list and effects changed event
    m_EffectsLock.Enter();
    effectsLocked = TRUE;

    // Always close existing effects change event handle
    if (m_hEffectsChangedEvent != NULL)
    {
        CloseHandle(m_hEffectsChangedEvent);
        m_hEffectsChangedEvent = NULL;
    }

    // If an event handle was specified, save it here (duplicated to control lifetime)
    if (Event != NULL)
    {
        if (!DuplicateHandle(GetCurrentProcess(), Event, GetCurrentProcess(), &m_hEffectsChangedEvent, EVENT_MODIFY_STATE, FALSE, 0))
        {
            hr = HRESULT_FROM_WIN32(GetLastError());
            goto Exit;
        }
    }

    // naked scope to force the initialization of list[] to be after we enter the critical section
    {
        struct EffectControl
        {
            GUID effect;
            BOOL control;
        };

        EffectControl list[] =
        {
            { SwapEffectId, TRUE },
        };

        if (!IsEqualGUID(m_AudioProcessingMode, AUDIO_SIGNALPROCESSINGMODE_RAW))
        {
            // count the active effects
            for (UINT i = 0; i < ARRAYSIZE(list); i++)
            {
                if (list[i].control)
                {
                    cEffects++;
                }
            }
        }

        if (0 == cEffects)
        {
            *ppEffectsIds = NULL;
            *pcEffects = 0;
        }
        else
        {
            GUID *pEffectsIds = (LPGUID)CoTaskMemAlloc(sizeof(GUID) * cEffects);
            if (pEffectsIds == nullptr)
            {
                hr = E_OUTOFMEMORY;
                goto Exit;
            }

            // pick up the active effects
            UINT j = 0;
            for (UINT i = 0; i < ARRAYSIZE(list); i++)
            {
                if (list[i].control)
                {
                    pEffectsIds[j++] = list[i].effect;
                }
            }

            *ppEffectsIds = pEffectsIds;
            *pcEffects = cEffects;
        }

        hr = S_OK;
    }

Exit:
    if (effectsLocked)
    {
        m_EffectsLock.Leave();
    }
    return hr;
}

HRESULT CSwapAPOSFX::GetControllableSystemEffectsList(_Outptr_result_buffer_maybenull_(*numEffects) AUDIO_SYSTEMEFFECT** effects, _Out_ UINT* numEffects, _In_opt_ HANDLE event)
{
    RETURN_HR_IF_NULL(E_POINTER, effects);
    RETURN_HR_IF_NULL(E_POINTER, numEffects);

    *effects = nullptr;
    *numEffects = 0;

    // Always close existing effects change event handle
    if (m_hEffectsChangedEvent != NULL)
    {
        CloseHandle(m_hEffectsChangedEvent);
        m_hEffectsChangedEvent = NULL;
    }

    // If an event handle was specified, save it here (duplicated to control lifetime)
    if (event != NULL)
    {
        if (!DuplicateHandle(GetCurrentProcess(), event, GetCurrentProcess(), &m_hEffectsChangedEvent, EVENT_MODIFY_STATE, FALSE, 0))
        {
            RETURN_IF_FAILED(HRESULT_FROM_WIN32(GetLastError()));
        }
    }

    if (!IsEqualGUID(m_AudioProcessingMode, AUDIO_SIGNALPROCESSINGMODE_RAW))
    {
        wil::unique_cotaskmem_array_ptr<AUDIO_SYSTEMEFFECT> audioEffects(
            static_cast<AUDIO_SYSTEMEFFECT*>(CoTaskMemAlloc(NUM_OF_EFFECTS * sizeof(AUDIO_SYSTEMEFFECT))), NUM_OF_EFFECTS);
        RETURN_IF_NULL_ALLOC(audioEffects.get());

        for (UINT i = 0; i < NUM_OF_EFFECTS; i++)
        {
            audioEffects[i].id = m_effectInfos[i].id;
            audioEffects[i].state = m_effectInfos[i].state;
            audioEffects[i].canSetState = m_effectInfos[i].canSetState;
        }

        *numEffects = (UINT)audioEffects.size();
        *effects = audioEffects.release();
    }

    return S_OK;
}

HRESULT CSwapAPOSFX::SetAudioSystemEffectState(GUID effectId, AUDIO_SYSTEMEFFECT_STATE state)
{
    UNREFERENCED_PARAMETER(state);

    for (const auto& effectInfo : m_effectInfos)
    {
        if (effectId == effectInfo.id)
        {
            return E_NOTIMPL;
        }
    }

    return E_NOTFOUND;
}

//-------------------------------------------------------------------------
// Description:
//
//
//
// Parameters:
//
//
//
// Return values:
//
//
//
// Remarks:
//
//
HRESULT CSwapAPOSFX::OnPropertyValueChanged(LPCWSTR pwstrDeviceId, const PROPERTYKEY key)
{
    HRESULT     hr = S_OK;

    UNREFERENCED_PARAMETER(pwstrDeviceId);

    if (!m_spAPOSystemEffectsProperties)
    {
        return hr;
    }

    // If either the master disable or our APO's enable properties changed...
    if (PK_EQUAL(key, PKEY_Endpoint_Enable_Channel_Swap_SFX) ||
        PK_EQUAL(key, PKEY_AudioEndpoint_Disable_SysFx))
    {
        LONG nChanges = 0;

        // Synchronize access to the effects list and effects changed event
        m_EffectsLock.Enter();

        struct KeyControl
        {
            PROPERTYKEY key;
            LONG *value;
        };

        KeyControl controls[] =
        {
            { PKEY_Endpoint_Enable_Channel_Swap_SFX, &m_fEnableSwapSFX  },
        };

        for (int i = 0; i < ARRAYSIZE(controls); i++)
        {
            LONG fOldValue;
            LONG fNewValue = true;

            // Get the state of whether channel swap MFX is enabled or not
            fNewValue = GetCurrentEffectsSetting(m_spAPOSystemEffectsProperties, controls[i].key, m_AudioProcessingMode);

            // Swap in the new setting
            fOldValue = InterlockedExchange(controls[i].value, fNewValue);

            if (fNewValue != fOldValue)
            {
                nChanges++;
            }
        }

        // If anything changed and a change event handle exists
        if ((nChanges > 0) && (m_hEffectsChangedEvent != NULL))
        {
            SetEvent(m_hEffectsChangedEvent);
        }

        m_EffectsLock.Leave();
    }

    return hr;
}

HRESULT CSwapAPOSFX::GetApoNotificationRegistrationInfo(_Out_writes_(*count) APO_NOTIFICATION_DESCRIPTOR **apoNotifications, _Out_ DWORD *count)
{
    RETURN_HR_IF(E_POINTER, apoNotifications == nullptr || count == nullptr);

    *apoNotifications = nullptr;
    *count = 0;

    // Legacy initialization has no endpoint collection. Keep a valid empty
    // registration in that case; the render bridge remains at unity gain.
    if (m_audioEndpoint == nullptr ||
        m_microphoneBridgeRole != MicrophoneBridgeRole::Detached)
    {
        return S_OK;
    }

    wil::unique_cotaskmem_ptr<APO_NOTIFICATION_DESCRIPTOR[]> descriptors(
        static_cast<APO_NOTIFICATION_DESCRIPTOR*>(
            CoTaskMemAlloc(sizeof(APO_NOTIFICATION_DESCRIPTOR))));
    RETURN_IF_NULL_ALLOC(descriptors);
    RtlZeroMemory(descriptors.get(), sizeof(APO_NOTIFICATION_DESCRIPTOR));
    descriptors[0].type = APO_NOTIFICATION_TYPE_ENDPOINT_VOLUME;
    RETURN_IF_FAILED(m_audioEndpoint.query_to(&descriptors[0].audioEndpointVolume.device));

    *apoNotifications = descriptors.release();
    *count = 1;
    return S_OK;
}

void CSwapAPOSFX::HandleNotification(APO_NOTIFICATION *apoNotification)
{
    if (apoNotification == nullptr ||
        apoNotification->type != APO_NOTIFICATION_TYPE_ENDPOINT_VOLUME ||
        apoNotification->audioEndpointVolumeChange.volume == nullptr)
    {
        return;
    }

    const AUDIO_VOLUME_NOTIFICATION_DATA* volume =
        apoNotification->audioEndpointVolumeChange.volume;
    const float scalar = volume->fMasterVolume;
    if (!(scalar >= 0.0f && scalar <= 1.0f))
    {
        return;
    }

    const LONG gain = volume->bMuted
        ? 0
        : static_cast<LONG>(scalar * capyio::render_ring::kUnityGainMillion + 0.5f);
    InterlockedExchange(&m_endpointGainMillion, gain);
}

//-------------------------------------------------------------------------
// Description:
//
//  Destructor.
//
// Parameters:
//
//     void
//
// Return values:
//
//      void
//
// Remarks:
//
//      This method deletes whatever was allocated.
//
//      This method may not be called from a real-time processing thread.
//
CSwapAPOSFX::~CSwapAPOSFX(void)
{
    //
    // unregister for callbacks
    //
    if (m_bRegisteredEndpointNotificationCallback)
    {
        m_spEnumerator->UnregisterEndpointNotificationCallback(this);
    }

    if (m_hEffectsChangedEvent != NULL)
    {
        CloseHandle(m_hEffectsChangedEvent);
    }
} // ~CSwapAPOSFX
