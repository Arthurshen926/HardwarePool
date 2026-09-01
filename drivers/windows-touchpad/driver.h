#pragma once

#include <ntddk.h>
#include <wdf.h>
#include <wdmsec.h>
#include <vhf.h>

#include "broker_abi.h"
#include "reports.h"

typedef struct _CAPY_PTP_DEVICE_CONTEXT {
    VHFHANDLE VhfHandle;
    UCHAR InputMode;
    UCHAR FunctionSwitches;
} CAPY_PTP_DEVICE_CONTEXT, *PCAPY_PTP_DEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(CAPY_PTP_DEVICE_CONTEXT, CapyPtpGetContext);

typedef struct _CAPY_PTP_FILE_CONTEXT {
    BOOLEAN HelloAccepted;
    BOOLEAN Poisoned;
    ULONG NextSequence;
    ULONGLONG Generation;
    USHORT LastScanTime;
    UCHAR ActiveCount;
    CAPY_PTP_BROKER_CONTACT ActiveContacts[CAPY_PTP_BROKER_MAX_CONTACTS];
} CAPY_PTP_FILE_CONTEXT, *PCAPY_PTP_FILE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(CAPY_PTP_FILE_CONTEXT, CapyPtpGetFileContext);

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD CapyPtpEvtDeviceAdd;
EVT_WDF_OBJECT_CONTEXT_CLEANUP CapyPtpEvtDeviceCleanup;
EVT_WDF_DEVICE_FILE_CREATE CapyPtpEvtFileCreate;
EVT_WDF_FILE_CLOSE CapyPtpEvtFileClose;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL CapyPtpEvtIoDeviceControl;

EVT_VHF_ASYNC_OPERATION CapyPtpEvtGetFeature;
EVT_VHF_ASYNC_OPERATION CapyPtpEvtSetFeature;
