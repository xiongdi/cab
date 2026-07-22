; Add resources\bin to user PATH (HKCU) so cab-cli is available in terminal.
;
; NOTE: PATH entry is not removed on uninstall (stale entry is harmless).
; Users can clean up manually via System Properties → Environment Variables if needed.

!macro CAB_AddToPath
  ReadRegStr $0 HKCU "Environment" "Path"
  StrCpy $2 "$INSTDIR\resources\bin"
  ; Avoid duplicate entries
  ${If} $0 != ""
    StrCpy $1 "$0;$2"
  ${Else}
    StrCpy $1 "$2"
  ${EndIf}
  WriteRegExpandStr HKCU "Environment" "Path" "$1"
  ; Notify Windows about the environment change
  SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=500
!macroend
