Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!ifndef BUNDLE_ROOT
!error "BUNDLE_ROOT is required"
!endif
!ifndef OUTPUT_FILE
!error "OUTPUT_FILE is required"
!endif
!ifndef BUILD_ID
!error "BUILD_ID is required"
!endif
Name "CapyIO Local Debug"
VIProductVersion "0.5.24.0"
VIAddVersionKey "ProductName" "CapyIO Local Debug"
VIAddVersionKey "CompanyName" "Arthurshen926"
VIAddVersionKey "FileDescription" "CapyIO unsigned local test installer"
VIAddVersionKey "FileVersion" "0.5.24.0"
VIAddVersionKey "LegalCopyright" "CapyIO contributors"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\CapyIO\Applications\${BUILD_ID}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_WELCOMEPAGE_TITLE "CapyIO local test application"
!define MUI_WELCOMEPAGE_TEXT "Installs application files for this Windows account only.$\r$\n$\r$\nThis unsigned local test build is not a public release. Drivers, Broker service configuration and camera registration remain external prerequisites. Display/gamepad backends are optional. They are NOT installed or changed here.$\r$\n$\r$\nPairing and settings are preserved. Use Node > Exit node before switching versions. Closing the window leaves the node running."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TEXT "Application files installed. Open CapyIO Local Debug from the Start menu using your original paired Windows account.$\r$\n$\r$\nSystem components were not installed or reconfigured. The Android APK is in the android subdirectory.$\r$\n$\r$\nThis side-by-side version does not remove the previous installation."
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "English"
Section "Application files"
  IfFileExists "$INSTDIR" 0 new_directory
    MessageBox MB_OK|MB_ICONSTOP "Choose a new, non-existing version directory. Existing files will not be overwritten." /SD IDOK
    SetErrorLevel 2
    Abort
  new_directory:
  ClearErrors
  SetOutPath "$INSTDIR"
  File /r "${BUNDLE_ROOT}\*"
  IfErrors 0 copied
    MessageBox MB_OK|MB_ICONSTOP "Copy failed. Do not launch this incomplete version. Existing versions and pairing are unchanged." /SD IDOK
    SetErrorLevel 3
    Abort
  copied:
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/NOSHORTCUT" $1
  IfErrors create_shortcut done
  create_shortcut:
  SetShellVarContext current
  CreateShortCut "$SMPROGRAMS\CapyIO Local Debug.lnk" "$INSTDIR\capyio-desktop.exe" "" "$INSTDIR\capyio-desktop.exe"
  done:
SectionEnd
