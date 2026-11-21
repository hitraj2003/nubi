Unicode true
RequestExecutionLevel user
SetCompressor /SOLID lzma
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifdef TEST_INSTALLER
  !define PRODUCT "Nubi Native Installer Test"
!else
  !define PRODUCT "Nubi"
!endif
!define INSTALL_KEY "Software\Nubi\${PRODUCT}"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT}"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${PRODUCT}"
OutFile "${INSTALLER_OUT}"
InstallDir "$LOCALAPPDATA\${PRODUCT}"
InstallDirRegKey HKCU "${INSTALL_KEY}" ""
Icon "..\crates\nubi-app\assets\icon.ico"
UninstallIcon "..\crates\nubi-app\assets\icon.ico"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "FileDescription" "${PRODUCT} Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Nubi contributors"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Stage
Var InstalledApp
Var InstalledHelper
Var InstalledUninstaller
Var InstalledNotices
Var InstalledLicense

Function .onInit
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/P" $1
  ${IfNot} ${Errors}
    SetSilent silent
  ${EndIf}
FunctionEnd

!macro ReplaceFile name source flag
  ClearErrors
  IfFileExists "$INSTDIR\${name}" 0 +3
    Rename "$INSTDIR\${name}" "$Stage\old-${source}"
    IfErrors install_failed
  ClearErrors
  Rename "$Stage\${source}" "$INSTDIR\${name}"
  IfErrors install_failed
  StrCpy ${flag} 1
!macroend

!macro RestoreFile name source flag
  ${If} ${flag} == 1
    Delete "$INSTDIR\${name}"
  ${EndIf}
  IfFileExists "$Stage\old-${source}" 0 +2
    Rename "$Stage\old-${source}" "$INSTDIR\${name}"
!macroend

!macro CleanStageFile name
  Delete "$Stage\${name}"
  Delete "$Stage\old-${name}"
!macroend

Section "Nubi"
  ClearErrors
  CreateDirectory "$INSTDIR"
  IfErrors install_failed
  GetTempFileName $Stage "$INSTDIR"
  IfErrors install_failed
  Delete "$Stage"
  CreateDirectory "$Stage"
  IfErrors install_failed
  SetOutPath "$Stage"
  File "${PAYLOAD_DIR}\nubi.exe"
  File "${PAYLOAD_DIR}\nubi-hook.exe"
  File "..\LICENSE"
  File "..\THIRD_PARTY_NOTICES.md"
  WriteUninstaller "$Stage\uninstall.exe"
  IfErrors install_failed
  CreateDirectory "$INSTDIR\helpers"
  IfErrors install_failed

  ; Move the application last so a locked helper leaves the old app untouched.
  !insertmacro ReplaceFile "helpers\nubi-hook.exe" "nubi-hook.exe" $InstalledHelper
  !insertmacro ReplaceFile "uninstall.exe" "uninstall.exe" $InstalledUninstaller
  !insertmacro ReplaceFile "LICENSE" "LICENSE" $InstalledLicense
  !insertmacro ReplaceFile "THIRD_PARTY_NOTICES.md" "THIRD_PARTY_NOTICES.md" $InstalledNotices
  !insertmacro ReplaceFile "nubi.exe" "nubi.exe" $InstalledApp

  SetOutPath "$INSTDIR"
  WriteRegStr HKCU "${INSTALL_KEY}" "" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Nubi"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" '$\"$INSTDIR$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\nubi.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "MainBinaryName" "nubi.exe"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  CreateShortcut "$SMPROGRAMS\${PRODUCT}.lnk" "$INSTDIR\nubi.exe"
  !include "native-shortcut.nsh"
  !insertmacro SetNubiShortcutIdentity "$SMPROGRAMS\${PRODUCT}.lnk"

  !insertmacro CleanStageFile "nubi.exe"
  !insertmacro CleanStageFile "nubi-hook.exe"
  !insertmacro CleanStageFile "uninstall.exe"
  !insertmacro CleanStageFile "LICENSE"
  !insertmacro CleanStageFile "THIRD_PARTY_NOTICES.md"
  RMDir "$Stage"
  SetErrorLevel 0
  Goto installed

install_failed:
  StrCmp $Stage "" failed
  SetOutPath "$INSTDIR"
  !insertmacro RestoreFile "nubi.exe" "nubi.exe" $InstalledApp
  !insertmacro RestoreFile "THIRD_PARTY_NOTICES.md" "THIRD_PARTY_NOTICES.md" $InstalledNotices
  !insertmacro RestoreFile "LICENSE" "LICENSE" $InstalledLicense
  !insertmacro RestoreFile "uninstall.exe" "uninstall.exe" $InstalledUninstaller
  !insertmacro RestoreFile "helpers\nubi-hook.exe" "nubi-hook.exe" $InstalledHelper
  ; Delete staged payload only. Keep any backup that could not be restored.
  Delete "$Stage\nubi.exe"
  Delete "$Stage\nubi-hook.exe"
  Delete "$Stage\uninstall.exe"
  Delete "$Stage\LICENSE"
  Delete "$Stage\THIRD_PARTY_NOTICES.md"
  RMDir "$Stage"
failed:
  SetErrorLevel 2
  IfSilent +2
    MessageBox MB_OK|MB_ICONSTOP "Nubi could not be installed. Close Nubi and retry."
  Quit
installed:
SectionEnd

Function .onInstSuccess
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/R" $1
  ${IfNot} ${Errors}
    !ifdef TEST_INSTALLER
      ReadEnvStr $0 "NUBI_DEV_DATA_DIR"
      ${If} $0 != ""
        Exec '"$INSTDIR\nubi.exe" --dev --settings --exit-after=12'
      ${EndIf}
    !else
      Exec '"$INSTDIR\nubi.exe"'
    !endif
  ${EndIf}
FunctionEnd

Section "Uninstall"
  ; Keep the stable hook shim and user data. The registered command may still
  ; be serving a hook while the installer removes its packaged source.
  ClearErrors
  GetTempFileName $Stage "$INSTDIR"
  IfErrors uninstall_failed
  Delete "$Stage"
  CreateDirectory "$Stage"
  IfErrors uninstall_failed
  ClearErrors
  IfFileExists "$INSTDIR\helpers\nubi-hook.exe" 0 +3
    Rename "$INSTDIR\helpers\nubi-hook.exe" "$Stage\nubi-hook.exe"
    IfErrors uninstall_rollback
  IfFileExists "$INSTDIR\nubi.exe" 0 +3
    Rename "$INSTDIR\nubi.exe" "$Stage\nubi.exe"
    IfErrors uninstall_rollback
  Delete "$Stage\nubi.exe"
  Delete "$Stage\nubi-hook.exe"
  RMDir "$Stage"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR\helpers"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${PRODUCT}.lnk"
  DeleteRegValue HKCU "${RUN_KEY}" "${PRODUCT}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegValue HKCU "${INSTALL_KEY}" ""
  DeleteRegKey /ifempty HKCU "${INSTALL_KEY}"
  SetErrorLevel 0
  Goto uninstalled
uninstall_rollback:
  IfFileExists "$Stage\nubi-hook.exe" 0 +2
    Rename "$Stage\nubi-hook.exe" "$INSTDIR\helpers\nubi-hook.exe"
  RMDir "$Stage"
uninstall_failed:
  SetErrorLevel 2
  IfSilent +2
    MessageBox MB_OK|MB_ICONSTOP "Nubi is in use. Close it and retry."
  Quit
uninstalled:
SectionEnd
