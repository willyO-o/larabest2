use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::domain::slash;
use crate::error::AppError;

/// Vigila el primer nivel de una carpeta (no recursivo) y llama a
/// `on_change` una sola vez por ráfaga de cambios: copiar un proyecto genera
/// muchos eventos seguidos. El observador se detiene al soltar el valor
/// devuelto.
pub fn watch_first_level(
    dir: &Path,
    debounce: Duration,
    on_change: impl Fn() + Send + 'static,
) -> Result<RecommendedWatcher, AppError> {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
            ) {
                let _ = tx.send(());
            }
        }
    })
    .map_err(|e| AppError::io(format!("No se pudo vigilar {}", slash(dir)), e))?;
    watcher
        .watch(dir, RecursiveMode::NonRecursive)
        .map_err(|e| AppError::io(format!("No se pudo vigilar {}", slash(dir)), e))?;

    std::thread::spawn(move || {
        // Termina cuando se suelta el watcher (se cierra el canal).
        while rx.recv().is_ok() {
            loop {
                match rx.recv_timeout(debounce) {
                    Ok(()) => continue,
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
            on_change();
        }
    });
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;

    #[test]
    fn notifies_once_per_burst() {
        let dir = temp_dir("watch");
        let (tx, rx) = mpsc::channel();
        let _w = watch_first_level(&dir, Duration::from_millis(200), move || {
            let _ = tx.send(());
        })
        .unwrap();

        std::fs::create_dir(dir.join("blog")).unwrap();
        std::fs::create_dir(dir.join("tienda")).unwrap();

        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
