//! Menú www → Compartir en la red local: un proyecto se sirve también en
//! un puerto propio (8100, 8101…) en todas las interfaces, para abrirlo
//! desde otro equipo o el celular en la misma red con
//! `http://<IP del equipo>:<puerto>/`. Los dominios `.test` solo existen
//! en este equipo (archivo hosts): por eso va por IP y puerto.

use serde::Serialize;

use super::Core;
use crate::domain::{ServiceState, SharedSite, SHARE_PORTS};
use crate::error::AppError;

/// Lo que muestra el panel "Compartir en la red local".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    /// IP del equipo en la red local; `None` sin red.
    pub ip: Option<String>,
    /// El servidor web está en marcha (si no, nada se ve).
    pub web_running: bool,
    pub sites: Vec<ShareSite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSite {
    pub name: String,
    pub shared: bool,
    pub port: Option<u16>,
    /// `http://192.168.1.20:8100/` si está compartido y hay red.
    pub url: Option<String>,
    /// WordPress redirige a su dirección `.test`: el panel lo explica.
    pub wordpress: bool,
    /// Su wp-config.php ya sigue a la dirección usada (sitios nuevos).
    pub wordpress_ready: bool,
}

/// Marca que la app escribe en el wp-config.php de los WordPress nuevos.
pub const WP_LAN_MARK: &str = "LARABEST: red local";

/// Las mismas líneas de `templates/wp-config.php`, para los WordPress que
/// ya existían: abierto por IP, las URLs siguen a esa dirección.
const WP_LAN_SNIPPET: &str = r"
// LARABEST: red local. Abierto desde otro equipo por IP (Menú → www →
// Compartir en la red local), las URLs siguen a esa dirección en lugar de
// la del dominio .test, que solo existe en este equipo.
if ( isset( $_SERVER['HTTP_HOST'] ) && preg_match( '/^\d{1,3}(\.\d{1,3}){3}(:\d+)?$/', $_SERVER['HTTP_HOST'] ) ) {
    define( 'WP_HOME', 'http://' . $_SERVER['HTTP_HOST'] );
    define( 'WP_SITEURL', 'http://' . $_SERVER['HTTP_HOST'] );
}
";

impl Core {
    pub async fn share_info(&self) -> Result<ShareInfo, AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let ip = deps.ports.lan_ip();
        let web = cfg.web_server.service();
        let sites = self
            .sites()
            .await?
            .into_iter()
            .map(|site| {
                let port = cfg
                    .shared_sites
                    .iter()
                    .find(|s| s.name == site.name)
                    .map(|s| s.port);
                let wp_config = std::path::Path::new(&site.path).join("wp-config.php");
                let wordpress = deps.fs.exists(&wp_config);
                let wordpress_ready = wordpress
                    && deps
                        .fs
                        .read_to_string(&wp_config)
                        .is_ok_and(|c| c.contains(WP_LAN_MARK));
                ShareSite {
                    url: match (&ip, port) {
                        (Some(ip), Some(port)) => Some(format!("http://{ip}:{port}/")),
                        _ => None,
                    },
                    shared: port.is_some(),
                    port,
                    name: site.name,
                    wordpress,
                    wordpress_ready,
                }
            })
            .collect();
        Ok(ShareInfo {
            ip,
            web_running: self.supervisor.state(web) == ServiceState::Running,
            sites,
        })
    }

    /// Comparte o deja de compartir un proyecto. Al compartir: le da un
    /// puerto libre, abre esos puertos en el firewall de Windows (una vez,
    /// con permiso de administrador), regenera el servidor web y lo arranca
    /// o recarga.
    pub async fn set_shared(&self, name: &str, shared: bool) -> Result<ShareInfo, AppError> {
        let deps = self.supervisor.deps();
        if shared && !self.sites().await?.iter().any(|s| s.name == name) {
            return Err(AppError::NotFound(format!("No existe el proyecto {name}")));
        }
        let mut cfg = deps.config.load()?;
        let already = cfg.shared_sites.iter().any(|s| s.name == name);
        if shared == already {
            return self.share_info().await;
        }
        if shared {
            if deps.ports.lan_ip().is_none() {
                return Err(AppError::InvalidInput(
                    "El equipo no está conectado a ninguna red: conéctalo al Wi‑Fi o al cable y vuelve a intentarlo.".into(),
                ));
            }
            let port = self.free_share_port(&cfg)?;
            // Windows bloquea las conexiones de otros equipos: se abren los
            // puertos de compartir (pide permiso de administrador una vez).
            let ports = deps.ports.clone();
            tokio::task::spawn_blocking(move || ports.allow_lan_ports())
                .await
                .map_err(|e| AppError::Process(format!("No se pudo abrir el firewall: {e}")))??;
            cfg.shared_sites.push(SharedSite {
                name: name.to_string(),
                port,
            });
        } else {
            cfg.shared_sites.retain(|s| s.name != name);
        }
        deps.config.save(&cfg)?;
        self.apply_shares().await?;
        self.share_info().await
    }

    /// Agrega a un WordPress existente las líneas de "red local" (con una
    /// copia `wp-config.php.larabest.bak`), justo después de `<?php`.
    pub async fn fix_wordpress_for_lan(&self, name: &str) -> Result<ShareInfo, AppError> {
        let deps = self.supervisor.deps();
        let site = self
            .sites()
            .await?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No existe el proyecto {name}")))?;
        let file = std::path::Path::new(&site.path).join("wp-config.php");
        let content = deps.fs.read_to_string(&file)?;
        if content.contains(WP_LAN_MARK) {
            return self.share_info().await;
        }
        let Some(open) = content.find("<?php") else {
            return Err(AppError::InvalidInput(
                "Ese wp-config.php no empieza con <?php: agrega las líneas a mano.".into(),
            ));
        };
        let after = open + "<?php".len();
        let eol = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        // Con los saltos de línea del archivo (el código fuente puede
        // tener los de Windows o los de Unix según cómo se descargó).
        let snippet = WP_LAN_SNIPPET.replace("\r\n", "\n").replace('\n', eol);
        let fixed = format!("{}{snippet}{}", &content[..after], &content[after..]);
        deps.fs
            .write(&file.with_file_name("wp-config.php.larabest.bak"), &content)?;
        deps.fs.write(&file, &fixed)?;
        self.share_info().await
    }

    /// Un puerto de 8100–8199 que no use la app ni otro programa.
    fn free_share_port(&self, cfg: &crate::domain::AppConfig) -> Result<u16, AppError> {
        let deps = self.supervisor.deps();
        let taken: Vec<u32> = cfg
            .ports()
            .owners(self.php_port_count())
            .into_iter()
            .map(|(port, _)| port)
            .collect();
        SHARE_PORTS
            .into_iter()
            .find(|&port| {
                !taken.contains(&u32::from(port))
                    && !cfg.shared_sites.iter().any(|s| s.port == port)
                    && deps.ports.is_free(port)
            })
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "No hay puertos libres entre {} y {} para compartir.",
                    SHARE_PORTS.start(),
                    SHARE_PORTS.end()
                ))
            })
    }

    /// Regenera el servidor web con los proyectos compartidos y lo arranca
    /// (si está detenido y hay alguno) o lo reinicia para que escuche en
    /// los puertos nuevos.
    async fn apply_shares(&self) -> Result<(), AppError> {
        let deps = self.supervisor.deps();
        let cfg = deps.config.load()?;
        let web = cfg.web_server.service();
        match self.supervisor.render(web).await {
            Ok(()) => {}
            Err(AppError::BinaryMissing { .. }) if cfg.shared_sites.is_empty() => return Ok(()),
            Err(e) => return Err(e),
        }
        match self.supervisor.state(web) {
            // Reinicio: abre y cierra puertos en Nginx y Apache por igual.
            ServiceState::Running => self.supervisor.restart(web).await,
            _ if !cfg.shared_sites.is_empty() => self.start(web).await,
            _ => Ok(()),
        }
    }
}
