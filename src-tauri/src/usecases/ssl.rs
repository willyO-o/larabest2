//! HTTPS opcional con la CA local.

use super::Core;
use crate::domain::ServiceState;
use crate::drivers::tools;
use crate::error::AppError;

impl Core {
    pub fn ssl_enabled(&self) -> bool {
        self.supervisor
            .deps()
            .config
            .load()
            .map(|c| c.ssl)
            .unwrap_or(false)
    }

    /// `true` si el sistema ya confía en la CA local.
    pub async fn ssl_trusted(&self) -> bool {
        self.ca.is_trusted().await
    }

    /// Activa o desactiva HTTPS. Al activarlo instala la CA en el sistema
    /// (Windows pide confirmar y macOS la contraseña, una vez) y emite un
    /// certificado por sitio. El servidor web se reinicia para comprobar
    /// que el puerto 443 esté libre.
    pub async fn set_ssl(&self, enabled: bool) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        if enabled {
            self.ca.ensure_ca()?;
            if !self.ca.is_trusted().await {
                self.ca.trust().await.map_err(|e| {
                    AppError::Process(format!(
                        "HTTPS sigue desactivado: no se instaló el certificado raíz. {e}"
                    ))
                })?;
            }
        }
        let mut cfg = deps.config.load()?;
        cfg.ssl = enabled;
        deps.config.save(&cfg)?;
        self.ensure_certs()?;

        let web = cfg.web_server.service();
        match self.supervisor.render(web).await {
            Ok(()) | Err(AppError::BinaryMissing { .. }) => {}
            Err(e) => return Err(e),
        }
        if self.supervisor.state(web) == ServiceState::Running {
            self.supervisor.restart(web).await?;
        }
        // Las URLs pasan a https:// (o vuelven a http://).
        let sites = self.sites().await?;
        deps.events.sites_changed(&sites);
        Ok(())
    }

    /// Con HTTPS activo, crea el certificado que falte (sitios, Adminer y
    /// phpMyAdmin).
    pub(super) fn ensure_certs(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        if !cfg.ssl {
            return Ok(());
        }
        let mut domains: Vec<String> =
            super::sites::scan_sites(deps.fs.as_ref(), &deps.paths, &cfg)
                .into_iter()
                .map(|s| s.domain)
                .collect();
        for tool in tools::ALL {
            if tools::installed(deps.fs.as_ref(), &deps.paths, tool) {
                domains.push(format!("{tool}.{}", cfg.tld));
            }
        }
        for domain in domains {
            self.ca.ensure_cert(&domain)?;
        }
        Ok(())
    }
}
