; Tauri owns installer files/shortcuts/ARP. User development data stays outside INSTDIR.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Projects, databases and runtimes are stored in the user's DEVONE Home."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Standard Tauri NSIS removes its product startup value only on a real
  ; uninstall, preserving it in /UPDATE mode. Do not duplicate that here.
  DetailPrint "DEVONE Home, projects, databases, credentials and CA data are preserved."
!macroend
