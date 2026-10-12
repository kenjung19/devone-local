; Tauri owns installer files/shortcuts/ARP. Home defaults to INSTDIR on Windows.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Projects, databases and runtimes default to the application folder (DEVONE Home)."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Standard Tauri NSIS removes its product startup value only on a real
  ; uninstall, preserving it in /UPDATE mode. Do not duplicate that here.
  DetailPrint "DEVONE Home, projects, databases, credentials and CA data are preserved."
!macroend
