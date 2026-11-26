; NSIS installer for Liebert Dynamic Island.
; Relative paths in NSIS are resolved from this script file, not the shell cwd.
; Build from the repository root (or pass absolute STAGING_DIR/OUT_FILE):
;   makensis /DAPP_VERSION=0.1.4 installer\windows\installer.nsi
; The staging directory must contain liebert-dynamic-island.exe, LICENSE.md and README.md.

!define APP_NAME "Liebert Dynamic Island"
!ifndef APP_VERSION
!define APP_VERSION "0.1.4"
!endif
!define APP_PUBLISHER "Zahwan316"
!define APP_EXE "liebert-dynamic-island.exe"
!define APP_KEY "Software\Liebert Dynamic Island"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Liebert Dynamic Island"

!ifndef STAGING_DIR
!define STAGING_DIR "..\..\dist\staging"
!endif
!ifndef OUT_FILE
!define OUT_FILE "..\..\dist\liebert-dynamic-island-windows-x86_64-setup.exe"
!endif

Name "${APP_NAME} ${APP_VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
InstallDirRegKey HKCU "${APP_KEY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

Page Directory
Page InstFiles

UninstPage UninstConfirm
UninstPage InstFiles

Section "Install"
  SetOutPath "$INSTDIR"
  File "${STAGING_DIR}\${APP_EXE}"
  File "${STAGING_DIR}\LICENSE.md"
  File "${STAGING_DIR}\README.md"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${APP_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME} ${APP_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\LICENSE.md"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"

  DeleteRegKey HKCU "${APP_KEY}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
