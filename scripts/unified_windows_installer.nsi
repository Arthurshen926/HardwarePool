Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!ifndef BUNDLE_ROOT
!error "BUNDLE_ROOT required"
!endif
Name "CapyIO 本地 Alpha"
OutFile "${OUTPUT_FILE}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_WELCOMEPAGE_TITLE "安装 CapyIO 本地 Alpha"
!define MUI_WELCOMEPAGE_TEXT "统一安装应用、后台服务及系统摄像头组件。安装时会请求管理员确认，日常使用无需管理员权限。$\r$\n$\r$\n请先在 CapyIO 中退出节点，仅关闭窗口不够。已有配对与设置会保留。$\r$\n$\r$\n已有健康驱动将保留。缺失驱动时，仅带有经核验签名驱动包的安装器可以自动安装，否则会停止并提示缺失材料。不会修改系统启动安全设置或证书信任。"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TEXT "应用、服务与摄像头组件已安装。请从开始菜单启动 CapyIO；安装程序不会以管理员身份启动应用。$\r$\n$\r$\n开始菜单同时提供状态检查、回滚及恢复原系统配置入口。旧版本文件暂留作恢复缓存，配对和用户设置不删除。$\r$\n$\r$\n这是未作生产签名的本地 Alpha，不是已通过新机器验收的正式发行版。"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "SimpChinese"
Section
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "需要 64 位 Windows。"
    Abort
  ${EndIf}
  SetRegView 64
  InitPluginsDir
  SetOutPath "$PLUGINSDIR\payload"
  File /r "${BUNDLE_ROOT}\*"
  nsExec::ExecToStack '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\payload\manage_windows_installation.ps1" -Mode Install -PackageRoot "$PLUGINSDIR\payload"'
  Pop $0
  Pop $1
  ${If} $0 == 3010
    MessageBox MB_OK|MB_ICONINFORMATION "驱动需要重启后才能完成。请手动重启电脑，再运行本安装器；当前尚未完成应用配置。"
    SetErrorLevel 3010
    Abort
  ${EndIf}
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONSTOP "安装未完成（代码 $0）。$\r$\n$1$\r$\n请查看安装错误提示；不要启动未完成版本。"
    SetErrorLevel 1
    Abort
  ${EndIf}
  SetShellVarContext current
  CreateDirectory "$SMPROGRAMS\CapyIO Alpha"
  StrCpy $2 "$PROGRAMFILES64\CapyIO\${BUILD_ID}\manage_windows_installation.ps1"
  CreateShortCut "$SMPROGRAMS\CapyIO Alpha\CapyIO.lnk" "$WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "$2" -Mode Launch' "$PROGRAMFILES64\CapyIO\${BUILD_ID}\capyio-desktop.exe"
  CreateShortCut "$SMPROGRAMS\CapyIO Alpha\检查安装状态.lnk" "$WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -NoExit -ExecutionPolicy Bypass -File "$2" -Mode Check'
  CreateShortCut "$SMPROGRAMS\CapyIO Alpha\回滚上一个版本.lnk" "$WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -ExecutionPolicy Bypass -File "$2" -Mode Rollback'
  CreateShortCut "$SMPROGRAMS\CapyIO Alpha\恢复安装前系统配置.lnk" "$WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -ExecutionPolicy Bypass -File "$2" -Mode Remove'
SectionEnd
