pub mod adapters;
pub mod domain;
pub mod drivers;
pub mod error;
pub mod ports;

#[cfg(test)]
mod testing;

/// Atiende `--apply-hosts <archivo>`. Devuelve el código de salida si la app
/// se lanzó en modo auxiliar, o `None` para arrancar normalmente.
pub fn run_helper(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some("--apply-hosts") {
        return None;
    }
    let Some(file) = args.get(2) else {
        return Some(2);
    };
    #[cfg(windows)]
    {
        match adapters::windows::apply_hosts_request(std::path::Path::new(file)) {
            Ok(()) => Some(0),
            Err(e) => {
                eprintln!("{e}");
                Some(1)
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("--apply-hosts solo está disponible en Windows ({file})");
        Some(1)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error al iniciar la aplicación");
}

#[cfg(test)]
mod tests {
    use super::run_helper;

    #[test]
    fn helper_mode_only_with_flag() {
        assert_eq!(run_helper(&["app".into()]), None);
        assert_eq!(run_helper(&["app".into(), "--apply-hosts".into()]), Some(2));
    }
}
