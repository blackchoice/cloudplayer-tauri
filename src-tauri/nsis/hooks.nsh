!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Setting directory permissions..."
  ExecWait 'icacls "$INSTDIR" /grant "Users:(OI)(CI)M" /T'
!macroend
