//! Crear un sitio con un clic: en blanco, WordPress o Laravel.

use std::path::Path;
use std::time::Duration;

use minijinja::context;

use super::sites::site_url;
use super::Core;
use crate::domain::{
    database_name, is_valid_site_name, set_env, slash, ProgressState, ServiceId, ServiceState,
    Site, SitePreset, SiteProgress,
};
use crate::drivers::{mariadb, nginx, php, templates, RenderContext};
use crate::error::AppError;

/// WordPress en español; se descarga una vez y se reutiliza.
pub const WORDPRESS_URL: &str = "https://es.wordpress.org/latest-es_ES.zip";
pub const COMPOSER_URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar";

const WORDPRESS_ZIP: &str = "wordpress-es_ES.zip";
const CREATE_PROJECT_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const ARTISAN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Líneas de salida de Composer que se muestran si falla.
const OUTPUT_TAIL: usize = 8;

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(OUTPUT_TAIL)..].join("\n")
}

/// Avisa a la interfaz de cada paso de la creación.
struct Progress<'a> {
    core: &'a Core,
    name: &'a str,
    preset: SitePreset,
}

impl Progress<'_> {
    fn emit(&self, step: impl Into<String>, state: ProgressState, detail: Option<String>) {
        self.core
            .supervisor
            .deps()
            .events
            .site_progress(&SiteProgress {
                name: self.name.to_string(),
                preset: self.preset,
                step: step.into(),
                state,
                detail,
            });
    }

    fn step(&self, step: impl Into<String>) {
        self.emit(step, ProgressState::Running, None);
    }
}

impl Core {
    /// Crea `www/<name>` con la plantilla elegida, su base de datos si hace
    /// falta, y lo da de alta (vhost, archivo hosts y certificado).
    pub async fn create_site(&self, name: &str, preset: SitePreset) -> Result<Site, AppError> {
        let Ok(_guard) = self.create_lock.try_lock() else {
            return Err(AppError::InvalidInput(
                "Ya se está creando otro sitio. Espera a que termine.".into(),
            ));
        };
        let deps = self.supervisor.deps();
        if !is_valid_site_name(name) {
            return Err(AppError::InvalidInput(
                "El nombre solo puede tener letras, números y guiones (sin espacios ni puntos)."
                    .into(),
            ));
        }
        if name.eq_ignore_ascii_case(nginx::ADMINER_SITE) {
            return Err(AppError::InvalidInput(
                "El nombre «adminer» está reservado para la base de datos.".into(),
            ));
        }
        let dir = deps.paths.www().join(name);
        let taken = deps
            .fs
            .list_dirs(&deps.paths.www())
            .unwrap_or_default()
            .iter()
            .any(|d| d.eq_ignore_ascii_case(name));
        if taken || deps.fs.exists(&dir) {
            return Err(AppError::InvalidInput(format!(
                "Ya existe la carpeta {}.",
                slash(&dir)
            )));
        }

        let progress = Progress {
            core: self,
            name,
            preset,
        };
        let created = match preset {
            SitePreset::Blank => self.create_blank(name, &dir, &progress),
            SitePreset::Wordpress => self.create_wordpress(name, &dir, &progress).await,
            SitePreset::Laravel => self.create_laravel(name, &dir, &progress).await,
        };
        let warning = match created {
            Ok(warning) => warning,
            Err(e) => {
                // Lo creado a medias se borra para poder reintentar.
                let _ = deps.fs.remove_dir_all(&dir);
                progress.emit(
                    "No se pudo crear el sitio",
                    ProgressState::Failed,
                    Some(e.to_string()),
                );
                return Err(e);
            }
        };

        progress.step("Configurando Nginx y el archivo hosts");
        // Si se cancela el permiso del archivo hosts el sitio igual existe:
        // queda marcado como pendiente y se puede reintentar.
        let sites = match self.sync_sites().await {
            Ok(sites) => sites,
            Err(_) => self.sites().await?,
        };
        let site = sites
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(format!("No se encontró el sitio {name}")))?;
        progress.emit("Sitio listo", ProgressState::Done, warning);
        Ok(site)
    }

    fn create_blank(
        &self,
        name: &str,
        dir: &Path,
        progress: &Progress<'_>,
    ) -> Result<Option<String>, AppError> {
        let deps = self.supervisor.deps();
        progress.step("Creando la carpeta del sitio");
        let index = templates::render(
            templates::BLANK_INDEX,
            context! {
                app_name => deps.app_name,
                name => name,
                path => slash(dir),
            },
        )?;
        deps.fs.write(&dir.join("index.php"), &index)?;
        Ok(None)
    }

    async fn create_wordpress(
        &self,
        name: &str,
        dir: &Path,
        progress: &Progress<'_>,
    ) -> Result<Option<String>, AppError> {
        let deps = self.supervisor.deps();
        let db = database_name(name);

        // Primero la base de datos: si MariaDB falla no se descarga nada.
        let ctx = self.database_ready(&db, progress).await?;

        let zip = deps.paths.downloads().join(WORDPRESS_ZIP);
        if deps.fs.exists(&zip) {
            progress.step("Usando la copia descargada de WordPress");
        } else {
            progress.step("Descargando WordPress en español");
            self.downloader.download(WORDPRESS_URL, &zip).await?;
        }

        progress.step("Extrayendo WordPress");
        let archive = self.archive.clone();
        let (zip_path, dest) = (zip.clone(), dir.to_path_buf());
        tokio::task::spawn_blocking(move || archive.extract_zip(&zip_path, &dest, true))
            .await
            .map_err(|e| AppError::Process(format!("La extracción se interrumpió: {e}")))??;
        if !deps.fs.exists(&dir.join("wp-settings.php")) {
            // Una descarga dañada no debe reutilizarse en el próximo intento.
            let _ = deps.fs.remove_file(&zip);
            return Err(AppError::Process(
                "El archivo descargado no es un WordPress válido. Vuelve a intentarlo.".into(),
            ));
        }

        progress.step("Escribiendo wp-config.php");
        let keys: Vec<String> = (0..8).map(|_| self.secrets.random_string(64)).collect();
        let config = templates::render(
            templates::WP_CONFIG,
            context! {
                app_name => deps.app_name,
                db_name => db,
                db_port => ctx.config.mariadb_port,
                keys => keys,
            },
        )?;
        deps.fs.write(&dir.join("wp-config.php"), &config)?;
        Ok(None)
    }

    async fn create_laravel(
        &self,
        name: &str,
        dir: &Path,
        progress: &Progress<'_>,
    ) -> Result<Option<String>, AppError> {
        let deps = self.supervisor.deps();
        let db = database_name(name);

        // PHP de línea de comandos con el php.ini generado (extensiones).
        let www = deps.paths.www();
        php::cli_spec(&self.supervisor.context()?, &[], &www)?;
        self.supervisor.render(ServiceId::Php).await?;

        let ctx = self.database_ready(&db, progress).await?;

        let phar = deps.paths.composer_phar();
        if !deps.fs.exists(&phar) {
            progress.step("Descargando Composer");
            self.downloader.download(COMPOSER_URL, &phar).await?;
        }

        progress.step("Creando el proyecto Laravel (puede tardar varios minutos)");
        let args: Vec<String> = [
            &slash(&phar),
            "create-project",
            "laravel/laravel",
            name,
            "--prefer-dist",
            "--no-interaction",
            "--no-progress",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let spec = php::cli_spec(&ctx, &args, &www)?.log_to(deps.paths.logs().join("sites.log"));
        let out = ctx.runner.run(&spec, CREATE_PROJECT_TIMEOUT).await?;
        if !out.success() || !deps.fs.exists(&dir.join("artisan")) {
            return Err(AppError::Process(format!(
                "Composer no pudo crear el proyecto:\n{}",
                tail(&out.text())
            )));
        }

        progress.step("Configurando .env con MariaDB");
        let env_path = dir.join(".env");
        let mut env = deps
            .fs
            .read_to_string(&env_path)
            .or_else(|_| deps.fs.read_to_string(&dir.join(".env.example")))?;
        let url = site_url(
            &ctx.config,
            &format!("{}.{}", name.to_ascii_lowercase(), ctx.config.tld),
        );
        let port = ctx.config.mariadb_port.to_string();
        for (key, value) in [
            ("APP_URL", url.trim_end_matches('/')),
            ("DB_CONNECTION", "mariadb"),
            ("DB_HOST", "127.0.0.1"),
            ("DB_PORT", port.as_str()),
            ("DB_DATABASE", db.as_str()),
            ("DB_USERNAME", "root"),
            ("DB_PASSWORD", ""),
        ] {
            env = set_env(&env, key, value);
        }
        deps.fs.write(&env_path, &env)?;

        progress.step("Creando las tablas (php artisan migrate)");
        let migrate: Vec<String> = ["artisan", "migrate", "--force", "--no-interaction"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let spec = php::cli_spec(&ctx, &migrate, dir)?.log_to(deps.paths.logs().join("sites.log"));
        // El proyecto ya existe: si las migraciones fallan se avisa, no se borra.
        let warning = match ctx.runner.run(&spec, ARTISAN_TIMEOUT).await {
            Ok(out) if out.success() => None,
            Ok(out) => Some(format!(
                "Las migraciones fallaron; ejecuta «php artisan migrate» en la terminal.\n{}",
                tail(&out.text())
            )),
            Err(e) => Some(format!(
                "Las migraciones fallaron; ejecuta «php artisan migrate» en la terminal.\n{e}"
            )),
        };
        Ok(warning)
    }

    /// Arranca MariaDB si hace falta y crea la base de datos del sitio.
    async fn database_ready(
        &self,
        db: &str,
        progress: &Progress<'_>,
    ) -> Result<RenderContext, AppError> {
        if self.supervisor.state(ServiceId::Mariadb) != ServiceState::Running {
            progress.step("Iniciando MariaDB");
            self.start(ServiceId::Mariadb).await?;
        }
        if self.supervisor.state(ServiceId::Mariadb) != ServiceState::Running {
            let detail = self
                .supervisor
                .snapshot()
                .into_iter()
                .find(|s| s.id == ServiceId::Mariadb)
                .and_then(|s| s.detail)
                .unwrap_or_default();
            return Err(AppError::Process(format!(
                "MariaDB no está en marcha y el sitio necesita una base de datos. {detail}"
            )));
        }
        progress.step(format!("Creando la base de datos {db}"));
        let ctx = self.supervisor.context()?;
        mariadb::create_database(&ctx, db).await?;
        Ok(ctx)
    }
}
