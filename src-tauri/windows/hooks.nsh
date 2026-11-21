; Ganchos del instalador NSIS de Tauri.
;
; Antes de instalar o desinstalar, se le pide a LARABEST que se cierre en
; orden (`--quit`), como con "Salir": detiene los servicios (también la
; base de datos) antes de terminar. Así el instalador no tiene que cerrarla
; a la fuerza. Si no estaba abierta, no pasa nada.
;
; Al instalar, la app deja listo Windows con un solo aviso de administrador:
; runtime de Visual C++ para PHP, regla del firewall para compartir en la
; red local y permiso del usuario en el archivo hosts. Si ya estaba todo
; (al actualizar) no pregunta nada; si se cancela el aviso, la app pide
; permiso cuando lo necesita, como antes.
; Al desinstalar (no al actualizar) quita la regla y el permiso.

Var LarabestVerHi
Var LarabestVerLo
Var LarabestCanQuit

!macro NSIS_HOOK_PREINSTALL
  ; Solo si el LARABEST instalado ya entiende `--quit` (0.1.1 o posterior):
  ; uno anterior no lo conoce y se abriría en vez de cerrarse. En ese caso
  ; el instalador lo cierra como siempre.
  StrCpy $LarabestCanQuit 0
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    ClearErrors
    GetDLLVersion "$INSTDIR\${MAINBINARYNAME}.exe" $LarabestVerHi $LarabestVerLo
    ${IfNot} ${Errors}
      ; Hi = mayor * 65536 + menor; Lo = parche * 65536 + compilación.
      ${If} $LarabestVerHi > 1
        StrCpy $LarabestCanQuit 1
      ${ElseIf} $LarabestVerHi = 1
        ${If} $LarabestVerLo >= 65536
          StrCpy $LarabestCanQuit 1
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ${If} $LarabestCanQuit = 1
    DetailPrint "Cerrando LARABEST y deteniendo sus servicios..."
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Preparando Windows: Visual C++, firewall y archivo hosts..."
  ; La app explica en un mensaje qué va a hacer antes del aviso de
  ; Windows; en una instalación silenciosa (/S), sin mensaje.
  ${If} ${Silent}
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --system-setup --quiet'
  ${Else}
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --system-setup'
  ${EndIf}
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; El desinstalador es de la misma versión que la app instalada: entiende
  ; `--quit`. Se cierra en orden también al actualizar.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    DetailPrint "Cerrando LARABEST y deteniendo sus servicios..."
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    Pop $0
  ${EndIf}
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --system-cleanup'
    Pop $0
  ${EndIf}
!macroend
