// Evita la consola extra en Windows en builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Modo auxiliar elevado: escribe el bloque del archivo hosts y termina,
    // sin abrir ventana.
    if let Some(code) = app_lib::run_helper(&args) {
        std::process::exit(code);
    }
    app_lib::run();
}
