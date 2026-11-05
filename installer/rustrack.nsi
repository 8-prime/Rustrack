; Installs Rustrack and registers it as the "Rustrack" Windows service.
;
; Build with:
;   makensis "-DVERSION=0.1.0" "-DEXE=../target/release/backend.exe" installer/rustrack.nsi
;
; A relative EXE path is resolved from this directory, not the working directory.
; The quotes matter in PowerShell, which otherwise splits these at the first dot.
;
; Unattended install:
;   rustrack-setup-0.1.0.exe /S /LISTEN=127.0.0.1:8080
;
; The service itself is managed by `rustrack.exe service install|uninstall`, so
; this script only has to move files around and call those.

Unicode true

!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "x64.nsh"

!ifndef VERSION
  !error "Pass the version with -DVERSION=x.y.z"
!endif
!ifndef EXE
  !error "Pass the backend executable with -DEXE=path/to/backend.exe"
!endif

!define APP "Rustrack"
!define PROGRAM "rustrack.exe"
!define UNINSTALLER "uninstall.exe"
!define DEFAULT_LISTEN "0.0.0.0:3000"
!define REG_APP "Software\${APP}"
!define REG_UNINSTALL "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"

Name "${APP}"
OutFile "rustrack-setup-${VERSION}.exe"
InstallDir "$PROGRAMFILES64\${APP}"
InstallDirRegKey HKLM "${REG_APP}" "InstallDir"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show

Var ListenAddress
Var ListenField

!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_DIRECTORY
Page custom ListenPageCreate ListenPageLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "${APP} requires 64-bit Windows." /SD IDOK
    Abort
  ${EndIf}
  SetRegView 64

  ; An upgrade keeps the existing address unless told otherwise.
  ReadRegStr $ListenAddress HKLM "${REG_APP}" "ListenAddress"
  ${If} $ListenAddress == ""
    StrCpy $ListenAddress "${DEFAULT_LISTEN}"
  ${EndIf}

  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/LISTEN=" $1
  ${IfNot} ${Errors}
    StrCpy $ListenAddress $1
  ${EndIf}
FunctionEnd

Function un.onInit
  SetRegView 64
FunctionEnd

Function ListenPageCreate
  !insertmacro MUI_HEADER_TEXT "Network" "Choose where ${APP} serves its web interface."
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 24u "Address to listen on, as HOST:PORT. Use 127.0.0.1 as the host to allow connections from this computer only."
  Pop $0
  ${NSD_CreateText} 0 30u 100% 12u "$ListenAddress"
  Pop $ListenField
  nsDialogs::Show
FunctionEnd

Function ListenPageLeave
  ${NSD_GetText} $ListenField $ListenAddress
FunctionEnd

Section
  SetOutPath "$INSTDIR"

  ; Upgrading: the running service holds the executable open.
  ${If} ${FileExists} "$INSTDIR\${PROGRAM}"
    nsExec::ExecToLog '"$INSTDIR\${PROGRAM}" service uninstall'
    Pop $0
    ${If} $0 != 0
      MessageBox MB_ICONSTOP "Could not stop the existing ${APP} service (exit code $0)." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
  ${EndIf}

  File "/oname=${PROGRAM}" "${EXE}"
  WriteUninstaller "$INSTDIR\${UNINSTALLER}"

  WriteRegStr HKLM "${REG_APP}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${REG_APP}" "ListenAddress" "$ListenAddress"
  WriteRegStr HKLM "${REG_UNINSTALL}" "DisplayName" "${APP}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "Publisher" "${APP}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${REG_UNINSTALL}" "UninstallString" '"$INSTDIR\${UNINSTALLER}"'
  WriteRegStr HKLM "${REG_UNINSTALL}" "QuietUninstallString" '"$INSTDIR\${UNINSTALLER}" /S'
  WriteRegDWORD HKLM "${REG_UNINSTALL}" "NoModify" 1
  WriteRegDWORD HKLM "${REG_UNINSTALL}" "NoRepair" 1

  nsExec::ExecToLog '"$INSTDIR\${PROGRAM}" service install --listen "$ListenAddress"'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "The ${APP} service could not be started (exit code $0). See the details above." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog '"$INSTDIR\${PROGRAM}" service uninstall'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "Could not remove the ${APP} service (exit code $0)." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}

  Delete "$INSTDIR\${PROGRAM}"
  Delete "$INSTDIR\${UNINSTALLER}"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${REG_UNINSTALL}"
  DeleteRegKey HKLM "${REG_APP}"

  DetailPrint "Stored data was kept in %ProgramData%\${APP}."
SectionEnd
