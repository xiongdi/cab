; CAB NSIS installer hooks
!include "path.nsh"

!macro customInstall
  ; Add install directory to user PATH
  !insertmacro CAB_AddToPath
!macroend

