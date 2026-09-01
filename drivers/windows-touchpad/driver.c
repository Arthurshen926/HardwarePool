#include <initguid.h>

#include "driver.h"

DEFINE_GUID(
    GUID_DEVINTERFACE_CAPYIO_VHF_TOUCHPAD,
    0x398a3698, 0x9c4f, 0x4be2, 0x9a, 0xd2, 0x0e, 0xd8, 0xdf, 0x9b, 0x71, 0x31);

static BOOLEAN
CapyPtpBytesAreZero(_In_reads_bytes_(Length) const UCHAR* Bytes, _In_ SIZE_T Length)
{
    SIZE_T index;

    for (index = 0U; index < Length; ++index) {
        if (Bytes[index] != 0U) {
            return FALSE;
        }
    }
    return TRUE;
}

static NTSTATUS
CapyPtpCopyFeature(
    _In_ PHID_XFER_PACKET Packet,
    _In_reads_bytes_(SourceLength) const VOID* Source,
    _In_ ULONG SourceLength
    )
{
    if (Packet == NULL || Packet->reportBuffer == NULL ||
        Packet->reportBufferLen < SourceLength) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    RtlCopyMemory(Packet->reportBuffer, Source, SourceLength);
    return STATUS_SUCCESS;
}

static const CAPY_PTP_BROKER_CONTACT*
CapyPtpFindContact(
    _In_reads_(Count) const CAPY_PTP_BROKER_CONTACT* Contacts,
    _In_ UCHAR Count,
    _In_ UCHAR ContactId
    )
{
    UCHAR index;

    for (index = 0U; index < Count; ++index) {
        if (Contacts[index].ContactId == ContactId) {
            return &Contacts[index];
        }
    }
    return NULL;
}

static NTSTATUS
CapyPtpSubmitContact(
    _In_ PCAPY_PTP_DEVICE_CONTEXT DeviceContext,
    _In_opt_ const CAPY_PTP_BROKER_CONTACT* Contact,
    _In_ UCHAR ContactCount,
    _In_ UCHAR Buttons,
    _In_ USHORT ScanTime
    )
{
    CAPY_PTP_INPUT_REPORT report = {0};
    HID_XFER_PACKET packet;

    report.ReportId = CAPY_PTP_REPORT_ID_TOUCH;
    report.ScanTime = ScanTime;
    report.ContactCount = ContactCount;
    report.Buttons = (UCHAR)(Buttons & CAPY_PTP_BROKER_BUTTONS_MASK);
    if (Contact != NULL) {
        report.ContactFlagsAndId = (UCHAR)(
            (Contact->Flags & CAPY_PTP_BROKER_CONTACT_FLAGS_MASK) |
            ((Contact->ContactId & 0x0fU) << 2U));
        report.X = Contact->X;
        report.Y = Contact->Y;
    }

    packet.reportBuffer = (PUCHAR)&report;
    packet.reportBufferLen = sizeof(report);
    packet.reportId = CAPY_PTP_REPORT_ID_TOUCH;
    return VhfReadReportSubmit(DeviceContext->VhfHandle, &packet);
}

static NTSTATUS
CapyPtpSubmitFrame(
    _In_ PCAPY_PTP_DEVICE_CONTEXT DeviceContext,
    _In_reads_(ContactCount) const CAPY_PTP_BROKER_CONTACT* Contacts,
    _In_ UCHAR ContactCount,
    _In_ UCHAR Buttons,
    _In_ USHORT ScanTime
    )
{
    UCHAR index;
    NTSTATUS status;

    if (ContactCount == 0U) {
        return CapyPtpSubmitContact(DeviceContext, NULL, 0U, Buttons, ScanTime);
    }

    for (index = 0U; index < ContactCount; ++index) {
        status = CapyPtpSubmitContact(
            DeviceContext,
            &Contacts[index],
            index == 0U ? ContactCount : 0U,
            Buttons,
            ScanTime);
        if (!NT_SUCCESS(status)) {
            return status;
        }
    }
    return STATUS_SUCCESS;
}

static NTSTATUS
CapyPtpReleaseActiveContacts(
    _In_ PCAPY_PTP_DEVICE_CONTEXT DeviceContext,
    _Inout_ PCAPY_PTP_FILE_CONTEXT FileContext,
    _In_ USHORT ScanTime
    )
{
    CAPY_PTP_BROKER_CONTACT releases[CAPY_PTP_BROKER_MAX_CONTACTS] = {0};
    UCHAR index;
    NTSTATUS status;

    if (FileContext->ActiveCount == 0U) {
        return STATUS_SUCCESS;
    }

    for (index = 0U; index < FileContext->ActiveCount; ++index) {
        releases[index] = FileContext->ActiveContacts[index];
        releases[index].Flags = (UCHAR)(
            releases[index].Flags & CAPY_PTP_BROKER_CONTACT_CONFIDENCE);
    }
    status = CapyPtpSubmitFrame(
        DeviceContext,
        releases,
        FileContext->ActiveCount,
        0U,
        ScanTime);
    if (NT_SUCCESS(status)) {
        RtlZeroMemory(FileContext->ActiveContacts, sizeof(FileContext->ActiveContacts));
        FileContext->ActiveCount = 0U;
        FileContext->LastScanTime = ScanTime;
    }
    return status;
}

static NTSTATUS
CapyPtpValidateData(_In_ const CAPY_PTP_BROKER_DATA* Data)
{
    UCHAR index;
    UCHAR prior;

    if (Data->ContactCount > CAPY_PTP_BROKER_MAX_CONTACTS ||
        (Data->Buttons & ~CAPY_PTP_BROKER_BUTTONS_MASK) != 0U) {
        return STATUS_INVALID_PARAMETER;
    }

    for (index = 0U; index < Data->ContactCount; ++index) {
        const CAPY_PTP_BROKER_CONTACT* contact = &Data->Contacts[index];
        if (contact->ContactId > 0x0fU || contact->X > 4095U ||
            contact->Y > 4095U ||
            (contact->Flags & ~CAPY_PTP_BROKER_CONTACT_FLAGS_MASK) != 0U ||
            (contact->Flags & CAPY_PTP_BROKER_CONTACT_TIP) == 0U) {
            return STATUS_INVALID_PARAMETER;
        }
        for (prior = 0U; prior < index; ++prior) {
            if (Data->Contacts[prior].ContactId == contact->ContactId) {
                return STATUS_DUPLICATE_OBJECTID;
            }
        }
    }

    if (!CapyPtpBytesAreZero(
            (const UCHAR*)&Data->Contacts[Data->ContactCount],
            (CAPY_PTP_BROKER_MAX_CONTACTS - Data->ContactCount) *
                sizeof(CAPY_PTP_BROKER_CONTACT))) {
        return STATUS_INVALID_PARAMETER;
    }
    return STATUS_SUCCESS;
}

static NTSTATUS
CapyPtpSubmitSnapshot(
    _In_ PCAPY_PTP_DEVICE_CONTEXT DeviceContext,
    _Inout_ PCAPY_PTP_FILE_CONTEXT FileContext,
    _In_ const CAPY_PTP_BROKER_DATA* Data
    )
{
    CAPY_PTP_BROKER_CONTACT priorFrame[CAPY_PTP_BROKER_MAX_CONTACTS] = {0};
    BOOLEAN hasRelease = FALSE;
    BOOLEAN hasAddition = FALSE;
    UCHAR index;
    NTSTATUS status;

    for (index = 0U; index < FileContext->ActiveCount; ++index) {
        const CAPY_PTP_BROKER_CONTACT* current = CapyPtpFindContact(
            Data->Contacts,
            Data->ContactCount,
            FileContext->ActiveContacts[index].ContactId);
        if (current != NULL) {
            priorFrame[index] = *current;
        } else {
            priorFrame[index] = FileContext->ActiveContacts[index];
            priorFrame[index].Flags = (UCHAR)(
                priorFrame[index].Flags & CAPY_PTP_BROKER_CONTACT_CONFIDENCE);
            hasRelease = TRUE;
        }
    }

    for (index = 0U; index < Data->ContactCount; ++index) {
        if (CapyPtpFindContact(
                FileContext->ActiveContacts,
                FileContext->ActiveCount,
                Data->Contacts[index].ContactId) == NULL) {
            hasAddition = TRUE;
        }
    }

    if (hasRelease) {
        status = CapyPtpSubmitFrame(
            DeviceContext,
            priorFrame,
            FileContext->ActiveCount,
            Data->Buttons,
            Data->ScanTime);
        if (!NT_SUCCESS(status)) {
            return status;
        }
        if (hasAddition) {
            status = CapyPtpSubmitFrame(
                DeviceContext,
                Data->Contacts,
                Data->ContactCount,
                Data->Buttons,
                (USHORT)(Data->ScanTime + 1U));
            if (!NT_SUCCESS(status)) {
                return status;
            }
        }
    } else {
        status = CapyPtpSubmitFrame(
            DeviceContext,
            Data->Contacts,
            Data->ContactCount,
            Data->Buttons,
            Data->ScanTime);
        if (!NT_SUCCESS(status)) {
            return status;
        }
    }

    RtlZeroMemory(FileContext->ActiveContacts, sizeof(FileContext->ActiveContacts));
    if (Data->ContactCount > 0U) {
        RtlCopyMemory(
            FileContext->ActiveContacts,
            Data->Contacts,
            Data->ContactCount * sizeof(CAPY_PTP_BROKER_CONTACT));
    }
    FileContext->ActiveCount = Data->ContactCount;
    FileContext->LastScanTime =
        (USHORT)(Data->ScanTime + ((hasRelease && hasAddition) ? 1U : 0U));
    return STATUS_SUCCESS;
}

static VOID
CapyPtpBuildAck(
    _Out_ PCAPY_PTP_BROKER_RECORD Record,
    _In_ ULONG AcceptedSequence
    )
{
    RtlZeroMemory(Record, sizeof(*Record));
    Record->Header.Magic = CAPY_PTP_BROKER_MAGIC;
    Record->Header.Version = CAPY_PTP_BROKER_VERSION;
    Record->Header.Kind = CapyPtpBrokerAck;
    Record->Header.PayloadLength = sizeof(CAPY_PTP_BROKER_ACK);
    Record->Header.Sequence = AcceptedSequence;
    Record->Payload.Ack.AcceptedSequence = AcceptedSequence;
}

VOID
CapyPtpEvtIoDeviceControl(
    _In_ WDFQUEUE Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t OutputBufferLength,
    _In_ size_t InputBufferLength,
    _In_ ULONG IoControlCode
    )
{
    PCAPY_PTP_BROKER_RECORD record;
    PCAPY_PTP_BROKER_RECORD output;
    PCAPY_PTP_DEVICE_CONTEXT deviceContext;
    PCAPY_PTP_FILE_CONTEXT fileContext;
    WDFFILEOBJECT fileObject;
    size_t length;
    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;

    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    if (IoControlCode != IOCTL_CAPY_PTP_BROKER_RECORD) {
        WdfRequestComplete(Request, status);
        return;
    }

    status = WdfRequestRetrieveInputBuffer(
        Request,
        sizeof(CAPY_PTP_BROKER_RECORD),
        (PVOID*)&record,
        &length);
    if (!NT_SUCCESS(status) || length != sizeof(CAPY_PTP_BROKER_RECORD)) {
        WdfRequestComplete(Request, NT_SUCCESS(status) ? STATUS_INFO_LENGTH_MISMATCH : status);
        return;
    }
    status = WdfRequestRetrieveOutputBuffer(
        Request,
        sizeof(CAPY_PTP_BROKER_RECORD),
        (PVOID*)&output,
        &length);
    if (!NT_SUCCESS(status) || length < sizeof(CAPY_PTP_BROKER_RECORD)) {
        WdfRequestComplete(Request, NT_SUCCESS(status) ? STATUS_BUFFER_TOO_SMALL : status);
        return;
    }

    if (record->Header.Magic != CAPY_PTP_BROKER_MAGIC ||
        record->Header.Version != CAPY_PTP_BROKER_VERSION) {
        WdfRequestComplete(Request, STATUS_REVISION_MISMATCH);
        return;
    }

    fileObject = WdfRequestGetFileObject(Request);
    fileContext = CapyPtpGetFileContext(fileObject);
    deviceContext = CapyPtpGetContext(WdfIoQueueGetDevice(Queue));

    switch (record->Header.Kind) {
    case CapyPtpBrokerHello:
        if (fileContext->HelloAccepted || fileContext->Poisoned ||
            record->Header.Sequence != 0U ||
            record->Header.PayloadLength != sizeof(CAPY_PTP_BROKER_HELLO) ||
            record->Payload.Hello.Generation == 0U ||
            !CapyPtpBytesAreZero(
                &record->Payload.Bytes[sizeof(CAPY_PTP_BROKER_HELLO)],
                sizeof(record->Payload.Bytes) - sizeof(CAPY_PTP_BROKER_HELLO))) {
            status = STATUS_INVALID_PARAMETER;
            break;
        }
        fileContext->HelloAccepted = TRUE;
        fileContext->Generation = record->Payload.Hello.Generation;
        fileContext->NextSequence = 1U;
        status = STATUS_SUCCESS;
        break;

    case CapyPtpBrokerData:
        if (!fileContext->HelloAccepted || fileContext->Poisoned) {
            status = STATUS_INVALID_DEVICE_STATE;
            break;
        }
        if (record->Header.PayloadLength != sizeof(CAPY_PTP_BROKER_DATA) ||
            record->Header.Sequence != fileContext->NextSequence) {
            status = STATUS_REQUEST_OUT_OF_SEQUENCE;
            break;
        }
        if (fileContext->NextSequence == MAXULONG) {
            status = STATUS_INTEGER_OVERFLOW;
            break;
        }
        status = CapyPtpValidateData(&record->Payload.Data);
        if (!NT_SUCCESS(status)) {
            break;
        }
        status = CapyPtpSubmitSnapshot(
            deviceContext,
            fileContext,
            &record->Payload.Data);
        if (!NT_SUCCESS(status)) {
            fileContext->Poisoned = TRUE;
            break;
        }
        fileContext->NextSequence += 1U;
        break;

    case CapyPtpBrokerClose:
        if (!fileContext->HelloAccepted || fileContext->Poisoned ||
            record->Header.PayloadLength != 0U ||
            record->Header.Sequence != fileContext->NextSequence ||
            !CapyPtpBytesAreZero(record->Payload.Bytes, sizeof(record->Payload.Bytes))) {
            status = STATUS_INVALID_DEVICE_STATE;
            break;
        }
        status = CapyPtpReleaseActiveContacts(
            deviceContext,
            fileContext,
            (USHORT)(fileContext->LastScanTime + 1U));
        if (!NT_SUCCESS(status)) {
            fileContext->Poisoned = TRUE;
            break;
        }
        fileContext->HelloAccepted = FALSE;
        break;

    default:
        status = STATUS_NOT_SUPPORTED;
        break;
    }

    if (NT_SUCCESS(status)) {
        CapyPtpBuildAck(output, record->Header.Sequence);
        WdfRequestCompleteWithInformation(
            Request,
            STATUS_SUCCESS,
            sizeof(CAPY_PTP_BROKER_RECORD));
    } else {
        WdfRequestComplete(Request, status);
    }
}

VOID
CapyPtpEvtFileCreate(
    _In_ WDFDEVICE Device,
    _In_ WDFREQUEST Request,
    _In_ WDFFILEOBJECT FileObject
    )
{
    PCAPY_PTP_FILE_CONTEXT context = CapyPtpGetFileContext(FileObject);

    UNREFERENCED_PARAMETER(Device);
    RtlZeroMemory(context, sizeof(*context));
    WdfRequestComplete(Request, STATUS_SUCCESS);
}

VOID
CapyPtpEvtFileClose(_In_ WDFFILEOBJECT FileObject)
{
    WDFDEVICE device = WdfFileObjectGetDevice(FileObject);
    PCAPY_PTP_DEVICE_CONTEXT deviceContext = CapyPtpGetContext(device);
    PCAPY_PTP_FILE_CONTEXT fileContext = CapyPtpGetFileContext(FileObject);

    if (deviceContext->VhfHandle != NULL) {
        (VOID)CapyPtpReleaseActiveContacts(
            deviceContext,
            fileContext,
            (USHORT)(fileContext->LastScanTime + 1U));
    }
    RtlZeroMemory(fileContext, sizeof(*fileContext));
}

VOID
CapyPtpEvtGetFeature(
    _In_ PVOID VhfClientContext,
    _In_ VHFOPERATIONHANDLE VhfOperationHandle,
    _In_opt_ PVOID VhfOperationContext,
    _In_ PHID_XFER_PACKET HidTransferPacket
    )
{
    PCAPY_PTP_DEVICE_CONTEXT context =
        (PCAPY_PTP_DEVICE_CONTEXT)VhfClientContext;
    NTSTATUS status = STATUS_NOT_SUPPORTED;

    UNREFERENCED_PARAMETER(VhfOperationContext);

    if (HidTransferPacket == NULL) {
        (VOID)VhfAsyncOperationComplete(VhfOperationHandle, STATUS_INVALID_PARAMETER);
        return;
    }

    switch (HidTransferPacket->reportId) {
    case CAPY_PTP_REPORT_ID_CAPABILITIES: {
        const CAPY_PTP_CAPABILITIES_REPORT report = {
            CAPY_PTP_REPORT_ID_CAPABILITIES,
            CAPY_PTP_MAX_CONTACTS
        };
        status = CapyPtpCopyFeature(HidTransferPacket, &report, sizeof(report));
        break;
    }
    case CAPY_PTP_REPORT_ID_CERTIFICATION: {
        CAPY_PTP_CERTIFICATION_REPORT report = {0};
        report.ReportId = CAPY_PTP_REPORT_ID_CERTIFICATION;
        RtlCopyMemory(
            report.Blob,
            g_CapyPtpDefaultCertification,
            sizeof(report.Blob));
        status = CapyPtpCopyFeature(HidTransferPacket, &report, sizeof(report));
        break;
    }
    case CAPY_PTP_REPORT_ID_INPUT_MODE: {
        const CAPY_PTP_INPUT_MODE_REPORT report = {
            CAPY_PTP_REPORT_ID_INPUT_MODE,
            context->InputMode
        };
        status = CapyPtpCopyFeature(HidTransferPacket, &report, sizeof(report));
        break;
    }
    case CAPY_PTP_REPORT_ID_FUNCTION_SWITCH: {
        const CAPY_PTP_FUNCTION_SWITCH_REPORT report = {
            CAPY_PTP_REPORT_ID_FUNCTION_SWITCH,
            context->FunctionSwitches
        };
        status = CapyPtpCopyFeature(HidTransferPacket, &report, sizeof(report));
        break;
    }
    default:
        break;
    }

    (VOID)VhfAsyncOperationComplete(VhfOperationHandle, status);
}

VOID
CapyPtpEvtSetFeature(
    _In_ PVOID VhfClientContext,
    _In_ VHFOPERATIONHANDLE VhfOperationHandle,
    _In_opt_ PVOID VhfOperationContext,
    _In_ PHID_XFER_PACKET HidTransferPacket
    )
{
    PCAPY_PTP_DEVICE_CONTEXT context =
        (PCAPY_PTP_DEVICE_CONTEXT)VhfClientContext;
    NTSTATUS status = STATUS_NOT_SUPPORTED;

    UNREFERENCED_PARAMETER(VhfOperationContext);

    if (HidTransferPacket != NULL && HidTransferPacket->reportBuffer != NULL) {
        if (HidTransferPacket->reportId == CAPY_PTP_REPORT_ID_INPUT_MODE &&
            HidTransferPacket->reportBufferLen >= sizeof(CAPY_PTP_INPUT_MODE_REPORT)) {
            const PCAPY_PTP_INPUT_MODE_REPORT report =
                (PCAPY_PTP_INPUT_MODE_REPORT)HidTransferPacket->reportBuffer;
            if (report->InputMode <= 0x0aU) {
                context->InputMode = report->InputMode;
                status = STATUS_SUCCESS;
            } else {
                status = STATUS_INVALID_PARAMETER;
            }
        } else if (
            HidTransferPacket->reportId == CAPY_PTP_REPORT_ID_FUNCTION_SWITCH &&
            HidTransferPacket->reportBufferLen >= sizeof(CAPY_PTP_FUNCTION_SWITCH_REPORT)) {
            const PCAPY_PTP_FUNCTION_SWITCH_REPORT report =
                (PCAPY_PTP_FUNCTION_SWITCH_REPORT)HidTransferPacket->reportBuffer;
            context->FunctionSwitches = (UCHAR)(report->Switches & 0x03U);
            status = STATUS_SUCCESS;
        }
    }

    (VOID)VhfAsyncOperationComplete(VhfOperationHandle, status);
}

VOID
CapyPtpEvtDeviceCleanup(_In_ WDFOBJECT DeviceObject)
{
    PCAPY_PTP_DEVICE_CONTEXT context =
        CapyPtpGetContext((WDFDEVICE)DeviceObject);

    if (context->VhfHandle != NULL) {
        VhfDelete(context->VhfHandle, TRUE);
        context->VhfHandle = NULL;
    }
}

NTSTATUS
CapyPtpEvtDeviceAdd(
    _In_ WDFDRIVER Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
    )
{
    WDF_FILEOBJECT_CONFIG fileConfig;
    WDF_OBJECT_ATTRIBUTES fileAttributes;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    WDF_OBJECT_ATTRIBUTES queueAttributes;
    WDF_IO_QUEUE_CONFIG queueConfig;
    WDFDEVICE device;
    PCAPY_PTP_DEVICE_CONTEXT context;
    VHF_CONFIG vhfConfig;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(Driver);

    WdfDeviceInitSetCharacteristics(
        DeviceInit,
        FILE_AUTOGENERATED_DEVICE_NAME | FILE_DEVICE_SECURE_OPEN,
        TRUE);
    status = WdfDeviceInitAssignSDDLString(
        DeviceInit,
        &SDDL_DEVOBJ_SYS_ALL_ADM_ALL);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    WdfDeviceInitSetExclusive(DeviceInit, TRUE);

    WDF_FILEOBJECT_CONFIG_INIT(
        &fileConfig,
        CapyPtpEvtFileCreate,
        CapyPtpEvtFileClose,
        WDF_NO_EVENT_CALLBACK);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&fileAttributes, CAPY_PTP_FILE_CONTEXT);
    WdfDeviceInitSetFileObjectConfig(DeviceInit, &fileConfig, &fileAttributes);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(
        &deviceAttributes,
        CAPY_PTP_DEVICE_CONTEXT);
    deviceAttributes.EvtCleanupCallback = CapyPtpEvtDeviceCleanup;

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &device);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = WdfDeviceCreateDeviceInterface(
        device,
        &GUID_DEVINTERFACE_CAPYIO_VHF_TOUCHPAD,
        NULL);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(
        &queueConfig,
        WdfIoQueueDispatchSequential);
    queueConfig.EvtIoDeviceControl = CapyPtpEvtIoDeviceControl;
    WDF_OBJECT_ATTRIBUTES_INIT(&queueAttributes);
    queueAttributes.ExecutionLevel = WdfExecutionLevelPassive;
    status = WdfIoQueueCreate(device, &queueConfig, &queueAttributes, WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    context = CapyPtpGetContext(device);
    context->VhfHandle = NULL;
    context->InputMode = CAPY_PTP_INPUT_MODE_TOUCHPAD;
    context->FunctionSwitches =
        (UCHAR)(CAPY_PTP_SURFACE_SWITCH | CAPY_PTP_BUTTON_SWITCH);

    VHF_CONFIG_INIT(
        &vhfConfig,
        WdfDeviceWdmGetDeviceObject(device),
        g_CapyPtpReportDescriptorLength,
        g_CapyPtpReportDescriptor);
    vhfConfig.VendorID = 0x1209U;
    vhfConfig.ProductID = 0xc410U;
    vhfConfig.VersionNumber = 0x0001U;
    vhfConfig.VhfClientContext = context;
    vhfConfig.EvtVhfAsyncOperationGetFeature = CapyPtpEvtGetFeature;
    vhfConfig.EvtVhfAsyncOperationSetFeature = CapyPtpEvtSetFeature;

    status = VhfCreate(&vhfConfig, &context->VhfHandle);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = VhfStart(context->VhfHandle);
    if (!NT_SUCCESS(status)) {
        VhfDelete(context->VhfHandle, TRUE);
        context->VhfHandle = NULL;
    }

    return status;
}

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PUNICODE_STRING RegistryPath
    )
{
    WDF_DRIVER_CONFIG config;

    WDF_DRIVER_CONFIG_INIT(&config, CapyPtpEvtDeviceAdd);
    return WdfDriverCreate(
        DriverObject,
        RegistryPath,
        WDF_NO_OBJECT_ATTRIBUTES,
        &config,
        WDF_NO_HANDLE);
}
