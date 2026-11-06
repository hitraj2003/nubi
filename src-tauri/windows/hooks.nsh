; The helper is built by beforeBuildCommand. Keep its source separate from the
; stable shim: the default install folder is also %LOCALAPPDATA%\Nubi.
!define NUBI_HOOK_SOURCE "${__FILEDIR__}\..\target\release\nubi-hook.exe"

!macro NSIS_HOOK_POSTINSTALL
  SetOutPath "$INSTDIR\helpers"
  File /oname=nubi-hook.exe "${NUBI_HOOK_SOURCE}"
  SetOutPath "$INSTDIR"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\helpers\nubi-hook.exe"
  RMDir "$INSTDIR\helpers"
!macroend
