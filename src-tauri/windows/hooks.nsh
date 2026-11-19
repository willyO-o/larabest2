; Ganchos del instalador NSIS de Tauri.
;
; Al instalar, la app deja listo Windows con un solo aviso de administrador:
; runtime de Visual C++ para PHP, regla del firewall para compartir en la
; red local y permiso del usuario en el archivo hosts. Si ya estaba todo
; (al actualizar) no pregunta nada; si se cancela el aviso, la app pide
; permiso cuando lo necesita, como antes.
; Al desinstalar (no al actualizar) quita la regla y el permiso.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Preparando Windows: Visual C++, firewall y archivo hosts..."
  nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --system-setup'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --system-cleanup'
    Pop $0
  ${EndIf}
!macroend
