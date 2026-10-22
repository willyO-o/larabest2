//! Progreso de una descarga hecha con un programa externo (curl,
//! PowerShell): el tamaño total sale de las cabeceras HTTP y lo descargado,
//! del archivo temporal que va creciendo.

use std::future::Future;
use std::path::Path;
use std::time::Duration;

use crate::error::AppError;

/// Cada cuánto se mide el archivo temporal.
const TICK: Duration = Duration::from_millis(400);

/// `Content-Length` de las cabeceras de `curl -sIL`. Con redirecciones hay
/// varias respuestas: vale la última que informa un tamaño.
pub fn parse_content_length(headers: &str) -> Option<u64> {
    headers
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<u64>().ok())
                .flatten()
        })
        .rfind(|n| *n > 0)
}

/// Corre la descarga y, mientras tanto, informa el tamaño de `part`. Al
/// terminar bien informa el tamaño final de `dest` (el 100 %).
pub async fn track_part<F>(
    download: F,
    part: &Path,
    dest: &Path,
    total: Option<u64>,
    on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<(), AppError>
where
    F: Future<Output = Result<(), AppError>>,
{
    tokio::pin!(download);
    let mut ticker = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            result = &mut download => {
                if result.is_ok() {
                    if let Ok(meta) = std::fs::metadata(dest) {
                        on_progress(meta.len(), Some(total.unwrap_or(meta.len())));
                    }
                }
                return result;
            }
            _ = ticker.tick() => {
                if let Ok(meta) = std::fs::metadata(part) {
                    on_progress(meta.len(), total);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::shared::temp_dir;
    use std::sync::Mutex;

    #[test]
    fn reads_the_size_of_the_last_response() {
        let headers = "HTTP/2 302\r\nlocation: https://x\r\ncontent-length: 0\r\n\r\nHTTP/2 200\r\nContent-Length: 29789003\r\ncontent-type: application/zip\r\n";
        assert_eq!(parse_content_length(headers), Some(29_789_003));
        assert_eq!(parse_content_length("HTTP/2 200\r\n"), None);
    }

    #[tokio::test]
    async fn reports_the_growing_part_file_and_the_final_size() {
        let dir = temp_dir("progress");
        let (part, dest) = (dir.join("a.part"), dir.join("a.zip"));
        let seen = Mutex::new(Vec::new());
        let report = |done: u64, total: Option<u64>| seen.lock().unwrap().push((done, total));
        let download = async {
            std::fs::write(&part, vec![0u8; 1000]).unwrap();
            tokio::time::sleep(Duration::from_millis(900)).await;
            std::fs::rename(&part, &dest).unwrap();
            Ok(())
        };
        track_part(download, &part, &dest, Some(2000), &report)
            .await
            .unwrap();
        let seen = seen.into_inner().unwrap();
        assert!(seen.contains(&(1000, Some(2000))), "{seen:?}");
        assert_eq!(seen.last(), Some(&(1000, Some(2000))));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
