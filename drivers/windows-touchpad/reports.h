#pragma once

#include <ntddk.h>

#define CAPY_PTP_REPORT_ID_TOUCH 1U
#define CAPY_PTP_REPORT_ID_CAPABILITIES 2U
#define CAPY_PTP_REPORT_ID_CERTIFICATION 3U
#define CAPY_PTP_REPORT_ID_INPUT_MODE 4U
#define CAPY_PTP_REPORT_ID_FUNCTION_SWITCH 5U

#define CAPY_PTP_MAX_CONTACTS 5U
#define CAPY_PTP_CERTIFICATION_BYTES 256U
#define CAPY_PTP_INPUT_MODE_TOUCHPAD 3U
#define CAPY_PTP_SURFACE_SWITCH 0x01U
#define CAPY_PTP_BUTTON_SWITCH 0x02U

#pragma pack(push, 1)

// Hybrid reporting: one fixed-size contact report per VHF submission. The
// first report in a scan carries total ContactCount; following reports carry 0.
typedef struct _CAPY_PTP_INPUT_REPORT {
    UCHAR ReportId;
    UCHAR ContactFlagsAndId; // confidence:1, tip:1, contact ID:4, reserved:2
    USHORT X;
    USHORT Y;
    USHORT ScanTime;
    UCHAR ContactCount;
    UCHAR Buttons;
} CAPY_PTP_INPUT_REPORT, *PCAPY_PTP_INPUT_REPORT;

typedef struct _CAPY_PTP_CAPABILITIES_REPORT {
    UCHAR ReportId;
    UCHAR MaximumCountAndPadType; // maximum count:4, pad type:4
} CAPY_PTP_CAPABILITIES_REPORT, *PCAPY_PTP_CAPABILITIES_REPORT;

typedef struct _CAPY_PTP_CERTIFICATION_REPORT {
    UCHAR ReportId;
    UCHAR Blob[CAPY_PTP_CERTIFICATION_BYTES];
} CAPY_PTP_CERTIFICATION_REPORT, *PCAPY_PTP_CERTIFICATION_REPORT;

typedef struct _CAPY_PTP_INPUT_MODE_REPORT {
    UCHAR ReportId;
    UCHAR InputMode;
} CAPY_PTP_INPUT_MODE_REPORT, *PCAPY_PTP_INPUT_MODE_REPORT;

typedef struct _CAPY_PTP_FUNCTION_SWITCH_REPORT {
    UCHAR ReportId;
    UCHAR Switches;
} CAPY_PTP_FUNCTION_SWITCH_REPORT, *PCAPY_PTP_FUNCTION_SWITCH_REPORT;

#pragma pack(pop)

C_ASSERT(sizeof(CAPY_PTP_INPUT_REPORT) == 10U);
C_ASSERT(sizeof(CAPY_PTP_CAPABILITIES_REPORT) == 2U);
C_ASSERT(sizeof(CAPY_PTP_CERTIFICATION_REPORT) == 257U);
C_ASSERT(sizeof(CAPY_PTP_INPUT_MODE_REPORT) == 2U);
C_ASSERT(sizeof(CAPY_PTP_FUNCTION_SWITCH_REPORT) == 2U);

extern UCHAR g_CapyPtpReportDescriptor[];
extern const USHORT g_CapyPtpReportDescriptorLength;
extern const UCHAR
    g_CapyPtpDefaultCertification[CAPY_PTP_CERTIFICATION_BYTES];
