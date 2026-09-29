; Tauri's NSIS template supplies the desktop shortcut and uninstall-data checkboxes.
; Only Switcher-owned files in .codex are removed; auth.json and backups stay intact.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode <> 1
    ${IfNot} ${Silent}
      ${If} $PassiveMode <> 1
        MessageBox MB_YESNO|MB_ICONQUESTION "Start CodexSwitcher automatically when you sign in to Windows?" IDNO +2
        WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}" '$"$INSTDIR\${MAINBINARYNAME}.exe$" --hidden'
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
    ${AndIf} $UpdateMode <> 1
    Delete "$PROFILE\.codex\switcher_config.dat"
    Delete "$PROFILE\.codex\switcher_accounts.dat"
    Delete "$PROFILE\.codex\switcher_config.json"
    Delete "$PROFILE\.codex\switcher_accounts_cache.json"
  ${EndIf}
!macroend
