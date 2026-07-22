; CAB NSIS installer hooks — service scope choice after files are installed.
; Yes = system (SCM): one UAC prompt; cab-cli install --scope system also starts the service.
; No = current user (Task Scheduler). Default button is No.

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $0 "$INSTDIR\cab-cli.exe"
  IfFileExists "$0" cab_cli_found 0
  StrCpy $0 "$INSTDIR\resources\bin\cab-cli.exe"
  IfFileExists "$0" cab_cli_found 0
  StrCpy $0 "$INSTDIR\_up_\resources\bin\cab-cli.exe"
  IfFileExists "$0" cab_cli_found 0
  StrCpy $0 "$INSTDIR\bin\cab-cli.exe"
  cab_cli_found:

  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 \
    "Install cab-srv service scope?$\r$\n$\r$\nYes = System (all users, starts at boot — will prompt for administrator)$\r$\nNo = Current user only (default, data in %USERPROFILE%\.cab)" \
    IDNO cab_scope_user
  ; Explicit UAC for system-scope (cab-cli also self-elevates if run without admin)
  nsExec::ExecToLog 'powershell -NoProfile -Command "Start-Process -FilePath \"$0\" -ArgumentList \"service\",\"install\",\"--scope\",\"system\" -Verb RunAs -Wait"'
  Goto cab_scope_done
  cab_scope_user:
  nsExec::ExecToLog '"$0" service install --scope user'
  nsExec::ExecToLog '"$0" start'
  cab_scope_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $0 "$INSTDIR\cab-cli.exe"
  IfFileExists "$0" cab_cli_u 0
  StrCpy $0 "$INSTDIR\resources\bin\cab-cli.exe"
  IfFileExists "$0" cab_cli_u 0
  StrCpy $0 "$INSTDIR\_up_\resources\bin\cab-cli.exe"
  IfFileExists "$0" cab_cli_u 0
  StrCpy $0 "$INSTDIR\bin\cab-cli.exe"
  cab_cli_u:
  nsExec::ExecToLog '"$0" service uninstall'
!macroend
