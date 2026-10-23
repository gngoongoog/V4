; BRAINTRADE V4 Enterprise V24 verified per-user desktop installer.
Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"
!ifndef PAYLOAD_DIR
  !error "Specify /DPAYLOAD_DIR=<verified portable directory>"
!endif
!ifndef OUTPUT_FILE
  !error "Specify /DOUTPUT_FILE=<installer EXE>"
!endif
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\BrainTrade-V4-Enterprise-V24"
Name "BRAINTRADE V4 Enterprise V24"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\BRAINTRADE V4"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show
BrandingText "BRAINTRADE V4 Windows x64"
VIProductVersion "0.8.24.0"
VIAddVersionKey "ProductName" "BRAINTRADE V4 Enterprise V24"
VIAddVersionKey "FileDescription" "BRAINTRADE V4 Desktop Installer"
VIAddVersionKey "FileVersion" "0.8.24.0"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

!macro CHECK_RUNNING
  ; Keep active-trade files untouched; do not kill trading processes.
  nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -Command "if (Get-Process -Name BrainTradeNext -ErrorAction SilentlyContinue) { exit 11 } else { exit 0 }"'
  Pop $0
  Pop $1
  StrCmp $0 "0" +3
    MessageBox MB_ICONEXCLAMATION|MB_OK "BRAINTRADE V4 is running. Stop trading safely and exit the application before installing or uninstalling. Check open positions first."
    Abort
!macroend

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "This setup requires Windows x64."
    Abort
  ${EndIf}
  !insertmacro CHECK_RUNNING
FunctionEnd

Section "Install BRAINTRADE V4"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /r "${PAYLOAD_DIR}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\BRAINTRADE V4"
  CreateShortCut "$SMPROGRAMS\BRAINTRADE V4\BRAINTRADE V4.lnk" "$INSTDIR\BrainTradeNext.exe" "" "$INSTDIR\BrainTradeNext.exe" 0
  CreateShortCut "$DESKTOP\BRAINTRADE V4.lnk" "$INSTDIR\BrainTradeNext.exe" "" "$INSTDIR\BrainTradeNext.exe" 0
  CreateShortCut "$SMPROGRAMS\BRAINTRADE V4\Uninstall BRAINTRADE V4.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "BRAINTRADE V4 Enterprise V24"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "BRAINTRADE"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "V24"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\BrainTradeNext.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Function un.onInit
  !insertmacro CHECK_RUNNING
FunctionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$DESKTOP\BRAINTRADE V4.lnk"
  Delete "$SMPROGRAMS\BRAINTRADE V4\BRAINTRADE V4.lnk"
  Delete "$SMPROGRAMS\BRAINTRADE V4\Uninstall BRAINTRADE V4.lnk"
  RMDir "$SMPROGRAMS\BRAINTRADE V4"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  RMDir /r "$INSTDIR"
  ; User trading and brain data in AppData and MT5 are untouched.
SectionEnd
