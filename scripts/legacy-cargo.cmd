@echo off
rem "Runner" de Tauri para el instalador legacy (scripts/build-legacy.mjs).
rem Tauri no acepta --target x86_64-win7-windows-msvc porque, al ser Tier 3,
rem no aparece en `rustup target list`. Aquí se compila para ese target con
rem nightly y -Z build-std, y el .exe se copia donde Tauri lo busca para
rem empaquetarlo (%CARGO_TARGET_DIR%\release).
cargo +nightly %* --target x86_64-win7-windows-msvc -Z build-std=std,panic_abort
if errorlevel 1 exit /b 1
if "%1"=="build" (
  copy /y "%CARGO_TARGET_DIR%\x86_64-win7-windows-msvc\release\larabest.exe" "%CARGO_TARGET_DIR%\release\larabest.exe" >nul || exit /b 1
)
