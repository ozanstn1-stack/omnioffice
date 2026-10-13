; OmniOffice NSIS installer hooks.
;
; Adds an Explorer "Send to → OmniOffice" entry so files can be sent straight
; to the app (they arrive through the existing launch-file queue, like a
; double-click). Removed on uninstall.

!macro NSIS_HOOK_POSTINSTALL
  CreateShortcut "$SENDTO\OmniOffice.lnk" "$INSTDIR\OmniOffice.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$SENDTO\OmniOffice.lnk"
!macroend
