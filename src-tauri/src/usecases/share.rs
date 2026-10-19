//! Compartir un sitio por internet (el "Share" de Laragon) con un túnel
//! rápido de Cloudflare: `cloudflared tunnel --url ...` da una dirección
//! `https://<algo>.trycloudflare.com` sin cuenta ni configuración. Sirve
//! para mostrar un avance o probar webhooks; se corta al dejar de
//! compartir, al detener todo o al salir.

use std::time::Duration;

use super::Core;
use crate::domain::catalog::App;
use crate::domain::{slash, ServiceState};
use crate::error::AppError;
use crate::ports::{tail_lines, ProcessSpec};

const URL_TIMEOUT: Duration = Duration::from_secs(40);

/// Un sitio compartido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub site: String,
    pub pid: u32,
    pub url: String,
}

/// La dirección pública que `cloudflared` escribe en su log.
pub fn tunnel_url(log: &str) -> Option<String> {
    let start = log.find("https://")?;
    log[start..].match_indices("https://").find_map(|(i, _)| {
        let rest = &log[start + i..];
        let url: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '-'))
            .collect();
        url.ends_with(".trycloudflare.com").then_some(url)
    })
}

impl Core {
    pub fn shares(&self) -> Vec<Share> {
        let runner = &self.supervisor.deps().runner;
        let mut shares = self.shares.lock().unwrap();
        shares.retain(|s| runner.is_alive(s.pid));
        shares.clone()
    }

    /// Comparte un sitio y devuelve su dirección pública.
    pub async fn share_site(&self, name: &str) -> Result<String, AppError> {
        if let Some(share) = self.shares().into_iter().find(|s| s.site == name) {
            return Ok(share.url);
        }
        let program = self.app_program(App::Cloudflared).ok_or_else(|| {
            AppError::NotFound(
                "Para compartir hace falta cloudflared: agrégalo en Menú → Agregar → cloudflared."
                    .into(),
            )
        })?;
        let web = self.web_server().service();
        if self.supervisor.state(web) != ServiceState::Running {
            return Err(AppError::InvalidInput(format!(
                "Inicia {} para compartir el sitio.",
                web.label()
            )));
        }
        let site = self
            .sites()
            .await?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No existe el sitio {name}")))?;
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let log = deps.paths.logs().join(format!("share-{name}.log"));
        let _ = deps.fs.remove_file(&log);
        // El túnel llega al servidor web por HTTP con el dominio del sitio.
        let spec = ProcessSpec::new(&program, deps.paths.root())
            .arg("tunnel")
            .arg("--no-autoupdate")
            .arg("--url")
            .arg(format!("http://127.0.0.1:{}", cfg.http_port))
            .arg("--http-host-header")
            .arg(&site.domain)
            .log_to(&log);
        let handle = deps.runner.spawn(&spec).await?;
        let started = std::time::Instant::now();
        let url = loop {
            if let Some(url) = deps
                .fs
                .read_to_string(&log)
                .ok()
                .and_then(|text| tunnel_url(&text))
            {
                break url;
            }
            if !deps.runner.is_alive(handle.pid) || started.elapsed() >= URL_TIMEOUT {
                let _ = deps.runner.stop_tree(handle.pid, Duration::ZERO).await;
                let detail = tail_lines(deps.fs.as_ref(), &log, 5).unwrap_or_default();
                return Err(AppError::Process(format!(
                    "No se pudo abrir el túnel (¿hay internet?). Log: {}\n{detail}",
                    slash(&log)
                )));
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        };
        self.shares.lock().unwrap().push(Share {
            site: name.to_string(),
            pid: handle.pid,
            url: url.clone(),
        });
        self.write_share_pids();
        Ok(url)
    }

    pub async fn stop_share(&self, name: &str) {
        let share = {
            let mut shares = self.shares.lock().unwrap();
            let found = shares.iter().position(|s| s.site == name);
            found.map(|i| shares.remove(i))
        };
        if let Some(share) = share {
            let _ = self
                .supervisor
                .deps()
                .runner
                .stop_tree(share.pid, Duration::from_secs(3))
                .await;
        }
        self.write_share_pids();
    }

    pub async fn stop_all_shares(&self) {
        let shares: Vec<Share> = std::mem::take(&mut *self.shares.lock().unwrap());
        for share in shares {
            let _ = self
                .supervisor
                .deps()
                .runner
                .stop_tree(share.pid, Duration::from_secs(3))
                .await;
        }
        self.write_share_pids();
    }

    fn write_share_pids(&self) {
        let deps = self.supervisor.deps();
        let file = deps.paths.tmp().join("share.pids");
        let content: String = self
            .shares
            .lock()
            .unwrap()
            .iter()
            .map(|s| format!("{} {}\n", s.pid, App::Cloudflared.portable_program()))
            .collect();
        if content.is_empty() {
            let _ = deps.fs.remove_file(&file);
        } else {
            let _ = deps.fs.write(&file, &content);
        }
    }

    /// Al abrir: corta los túneles que quedaron de una sesión anterior.
    pub(super) async fn cleanup_share_orphans(&self) {
        let deps = self.supervisor.deps();
        let file = deps.paths.tmp().join("share.pids");
        let Ok(content) = deps.fs.read_to_string(&file) else {
            return;
        };
        for line in content.lines() {
            let Some(pid) = line
                .split_whitespace()
                .next()
                .and_then(|p| p.parse::<u32>().ok())
            else {
                continue;
            };
            let is_tunnel = deps
                .runner
                .process_name(pid)
                .is_some_and(|n| n.to_ascii_lowercase().starts_with("cloudflared"));
            if is_tunnel {
                let _ = deps.runner.stop_tree(pid, Duration::from_secs(3)).await;
            }
        }
        let _ = deps.fs.remove_file(&file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FileStore;
    use crate::testing::{Harness, ROOT};
    use std::path::Path;

    #[test]
    fn finds_the_public_address_in_the_log() {
        let log = "2026-10-19T12:00:00Z INF Requesting new quick Tunnel on trycloudflare.com...\n\
            2026-10-19T12:00:02Z INF |  https://fuzzy-cat-river-moon.trycloudflare.com                     |\n";
        assert_eq!(
            tunnel_url(log).as_deref(),
            Some("https://fuzzy-cat-river-moon.trycloudflare.com")
        );
        assert_eq!(tunnel_url("INF https://api.trycloudflare.com/x no"), None);
        assert_eq!(tunnel_url("nada"), None);
    }

    #[tokio::test]
    async fn shares_a_site_through_cloudflared() {
        let h = Harness::new();
        let core = h.core();
        h.fs.create_dir_all(Path::new(&format!("{ROOT}/www/blog")))
            .unwrap();
        assert!(core.share_site("blog").await.is_err(), "sin cloudflared");
        h.fs.write(
            Path::new(&format!("{ROOT}/bin/cloudflared/cloudflared.exe")),
            "",
        )
        .unwrap();
        core.start_all().await.unwrap();
        // cloudflared escribe la dirección en su log al arrancar.
        h.fs.write(
            Path::new(&format!("{ROOT}/logs/share-blog.log")),
            "INF |  https://uno-dos-tres.trycloudflare.com  |",
        )
        .unwrap();
        let fs = h.fs.clone();
        let started = tokio::spawn({
            let fs = fs.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                fs.write(
                    Path::new(&format!("{ROOT}/logs/share-blog.log")),
                    "INF |  https://uno-dos-tres.trycloudflare.com  |",
                )
                .unwrap();
            }
        });
        let url = core.share_site("blog").await.unwrap();
        started.await.unwrap();
        assert_eq!(url, "https://uno-dos-tres.trycloudflare.com");
        let spec = h
            .runner
            .spawned()
            .into_iter()
            .find(|s| s.exe_name() == "cloudflared.exe")
            .unwrap();
        assert_eq!(
            spec.args,
            vec![
                "tunnel",
                "--no-autoupdate",
                "--url",
                "http://127.0.0.1:80",
                "--http-host-header",
                "blog.test"
            ]
        );
        assert_eq!(core.shares().len(), 1);
        core.stop_share("blog").await;
        assert!(core.shares().is_empty());
    }
}
