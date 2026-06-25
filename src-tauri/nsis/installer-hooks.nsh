; installer-hooks.nsh
; Registers AUMID and stub CLSID in the registry so that Windows Action Center
; can persist toasts after they dismiss (required for unpackaged Tauri apps).
;
; concept.md §8.4: "インストーラの責務"

!define GNWS_AUMID "dev.coxless.github-notifier-ws"
!define GNWS_STUB_CLSID "{B7BD96B7-F4FE-47FC-9E0B-A5E8C0D5E847}"

!macro CustomInstall
  ; -- AUMID registration (HKLM so all users share the same entry) --
  WriteRegStr HKLM "SOFTWARE\Classes\AppUserModelId\${GNWS_AUMID}" \
    "DisplayName" "github-notifier-ws"
  WriteRegStr HKLM "SOFTWARE\Classes\AppUserModelId\${GNWS_AUMID}" \
    "IconUri" "$INSTDIR\github-notifier-ws.exe"
  ; CustomActivator points to the stub CLSID; no actual COM server is present.
  WriteRegStr HKLM "SOFTWARE\Classes\AppUserModelId\${GNWS_AUMID}" \
    "CustomActivator" "${GNWS_STUB_CLSID}"

  ; -- Stub CLSID registration --
  ; Windows requires a registered CLSID entry for the AUMID even when no COM
  ; server is used. LocalServer32 set to the exe so the shell can find it.
  WriteRegStr HKLM "SOFTWARE\Classes\CLSID\${GNWS_STUB_CLSID}" \
    "" "github-notifier-ws"
  WriteRegStr HKLM "SOFTWARE\Classes\CLSID\${GNWS_STUB_CLSID}\LocalServer32" \
    "" "$INSTDIR\github-notifier-ws.exe"
!macroend

!macro CustomUninstall
  DeleteRegKey HKLM "SOFTWARE\Classes\AppUserModelId\${GNWS_AUMID}"
  DeleteRegKey HKLM "SOFTWARE\Classes\CLSID\${GNWS_STUB_CLSID}"
!macroend
