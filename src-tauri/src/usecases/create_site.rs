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
use crate::drivers::{mariadb, php, templates, tools, RenderContext};
use crate::error::AppError;

/// WordPress en español; se descarga una vez y se reutiliza.
pub const WORDPRESS_URL: &str = "https://es.wordpress.org/latest-es_ES.zip";
pub const COMPOSER_URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar";

const WORDPRESS_ZIP: &str = "wordpress-es_ES.zip";

/// `etc/sitios.conf` la primera vez: como el `sites.conf` de Laragon.
const SITE_TEMPLATES: &str = "# Plantillas propias de «Crear sitio» de {app}. Una por línea:\n\
#\n\
#   Nombre=acción\n\
#\n\
# La acción puede ser:\n\
#   - un comando de Composer: composer create-project paquete %s\n\
#   - la URL de un .zip, que se descomprime en www/<sitio>\n\
#   - cualquier otro comando, con el PATH de la Terminal: git clone URL %s\n\
# %s es el nombre del sitio. También se crea una base de datos con ese nombre.\n\
\n\
Symfony=composer create-project symfony/skeleton %s\n\
CodeIgniter 4=composer create-project codeigniter4/appstarter %s\n\
CakePHP=composer create-project --prefer-dist cakephp/app %s\n\
Drupal=composer create-project drupal/recommended-project %s\n\
# Slim=composer create-project slim/slim-skeleton %s\n\
# MiProyecto=git clone https://github.com/usuario/proyecto.git %s\n\
# MiPlantilla=https://example.com/plantilla.zip\n";

/// Una plantilla propia de `etc/sitios.conf`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteTemplate {
    pub name: String,
    pub action: String,
}

/// Lee `Nombre=acción`; ignora comentarios, vacías y nombres repetidos.
pub fn parse_templates(text: &str) -> Vec<SiteTemplate> {
    let mut out: Vec<SiteTemplate> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((name, action)) = line.split_once('=') else {
            continue;
        };
        let (name, action) = (name.trim(), action.trim());
        if name.is_empty() || name.len() > 40 || action.is_empty() {
            continue;
        }
        if !out.iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
            out.push(SiteTemplate {
                name: name.to_string(),
                action: action.to_string(),
            });
        }
    }
    out
}
const CREATE_PROJECT_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const ARTISAN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Líneas de salida de Composer que se muestran si falla.
const OUTPUT_TAIL: usize = 8;

/// Quita los códigos de color ANSI (los scripts de Laravel usan `--ansi`).
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn tail(text: &str) -> String {
    let clean = strip_ansi(text);
    let lines: Vec<&str> = clean.lines().filter(|l| !l.trim().is_empty()).collect();
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
        self.create_site_from(name, preset, None).await
    }

    /// Como `create_site`; con `SitePreset::Template`, `template` es el
    /// nombre de la plantilla propia de `etc/sitios.conf`.
    pub async fn create_site_from(
        &self,
        name: &str,
        preset: SitePreset,
        template: Option<&str>,
    ) -> Result<Site, AppError> {
        let template = match preset {
            SitePreset::Template => {
                let wanted = template.unwrap_or_default();
                Some(
                    self.site_templates()
                        .into_iter()
                        .find(|t| t.name == wanted)
                        .ok_or_else(|| {
                            AppError::NotFound(format!(
                                "No existe la plantilla «{wanted}» en etc/sitios.conf."
                            ))
                        })?,
                )
            }
            _ => None,
        };
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
        if tools::is_reserved(name) {
            return Err(AppError::InvalidInput(format!(
                "El nombre «{}» está reservado para la base de datos.",
                name.to_ascii_lowercase()
            )));
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
            SitePreset::Template => {
                let template = template.as_ref().expect("plantilla resuelta arriba");
                self.create_from_template(name, &dir, template, &progress)
                    .await
            }
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
        tokio::task::spawn_blocking(move || archive.extract_zip(&zip_path, &dest, None))
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
                db_password => ctx.config.root_password(ctx.database()),
                db_label => ctx.database().label(),
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

        let engine = ctx.database();
        progress.step(format!("Configurando .env con {}", engine.label()));
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
        let mut values: Vec<(&str, String)> = vec![
            ("APP_URL", url.trim_end_matches('/').to_string()),
            ("DB_CONNECTION", engine.key().into()),
            ("DB_HOST", "127.0.0.1".into()),
            ("DB_PORT", port),
            ("DB_DATABASE", db.clone()),
            ("DB_USERNAME", "root".into()),
            ("DB_PASSWORD", ctx.config.root_password(engine).to_string()),
        ];
        // Con Mailpit, el correo de Laravel se ve en su bandeja.
        if ctx.is_enabled(ServiceId::Mailpit) {
            values.extend([
                ("MAIL_MAILER", "smtp".to_string()),
                ("MAIL_HOST", "127.0.0.1".into()),
                ("MAIL_PORT", ctx.config.mailpit_smtp_port.to_string()),
            ]);
        }
        if ctx.is_enabled(ServiceId::Redis) {
            values.extend([
                ("REDIS_HOST", "127.0.0.1".to_string()),
                ("REDIS_PORT", ctx.config.redis_port.to_string()),
            ]);
        }
        for (key, value) in &values {
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

    /// Plantilla propia: un comando de Composer, la URL de un `.zip` o
    /// cualquier comando (por ejemplo `git clone`) con `%s` como nombre.
    /// También crea una base de datos con el nombre del sitio.
    async fn create_from_template(
        &self,
        name: &str,
        dir: &Path,
        template: &SiteTemplate,
        progress: &Progress<'_>,
    ) -> Result<Option<String>, AppError> {
        let deps = self.supervisor.deps();
        let db = database_name(name);
        let www = deps.paths.www();
        // La base de datos ayuda pero no es indispensable.
        let mut notes = Vec::new();
        match self.database_ready(&db, progress).await {
            Ok(ctx) => notes.push(format!(
                "Base de datos {db} creada en {} (usuario root).",
                ctx.database().label()
            )),
            Err(e) => notes.push(format!("No se creó la base de datos: {e}")),
        }
        let action = template.action.replace("%s", name);
        let log = deps.paths.logs().join("sites.log");
        let lower = action.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            progress.step(format!("Descargando {}", template.name));
            let zip = deps.paths.downloads().join(format!("plantilla-{name}.zip"));
            self.downloader.download(&action, &zip).await?;
            progress.step(format!("Extrayendo {}", template.name));
            let archive = self.archive.clone();
            let (zip_path, dest) = (zip.clone(), dir.to_path_buf());
            let extracted =
                tokio::task::spawn_blocking(move || archive.extract_zip(&zip_path, &dest, None))
                    .await
                    .map_err(|e| AppError::Process(format!("La extracción se interrumpió: {e}")))?;
            let _ = deps.fs.remove_file(&zip);
            extracted?;
        } else if let Some(rest) = action.strip_prefix("composer ") {
            let ctx = self.supervisor.context()?;
            php::cli_spec(&ctx, &[], &www)?;
            self.supervisor.render(ServiceId::Php).await?;
            let phar = deps.paths.composer_phar();
            if !deps.fs.exists(&phar) {
                progress.step("Descargando Composer");
                self.downloader.download(COMPOSER_URL, &phar).await?;
            }
            progress.step(format!(
                "Creando el proyecto {} (puede tardar varios minutos)",
                template.name
            ));
            let mut args = vec![slash(&phar)];
            args.extend(rest.split_whitespace().map(str::to_string));
            for extra in ["--no-interaction", "--no-progress"] {
                if !args.iter().any(|a| a == extra) {
                    args.push(extra.to_string());
                }
            }
            let spec = php::cli_spec(&ctx, &args, &www)?.log_to(&log);
            let out = ctx.runner.run(&spec, CREATE_PROJECT_TIMEOUT).await?;
            if !out.success() {
                return Err(AppError::Process(format!(
                    "Composer no pudo crear el proyecto:\n{}",
                    tail(&out.text())
                )));
            }
        } else {
            progress.step(format!(
                "Ejecutando la plantilla {} (puede tardar)",
                template.name
            ));
            let spec = self.shell_spec("sitios", name, &action, &www)?.log_to(&log);
            let out = deps.runner.run(&spec, CREATE_PROJECT_TIMEOUT).await?;
            if !out.success() {
                return Err(AppError::Process(format!(
                    "La plantilla falló:\n{}",
                    tail(&out.text())
                )));
            }
        }
        if !deps.fs.exists(dir) {
            return Err(AppError::Process(format!(
                "La plantilla no creó la carpeta {}. Usa %s en etc/sitios.conf para el nombre del sitio.",
                slash(dir)
            )));
        }
        Ok(Some(notes.join("\n")))
    }

    /// Plantillas propias de `etc/sitios.conf` (se crea con ejemplos).
    pub fn site_templates(&self) -> Vec<SiteTemplate> {
        let deps = self.supervisor.deps();
        let path = deps.paths.site_templates();
        if !deps.fs.exists(&path) {
            let _ = self.site_templates_path();
        }
        deps.fs
            .read_to_string(&path)
            .map(|text| parse_templates(&text))
            .unwrap_or_default()
    }

    pub fn site_templates_path(&self) -> Result<std::path::PathBuf, AppError> {
        let deps = self.supervisor.deps();
        let path = deps.paths.site_templates();
        if !deps.fs.exists(&path) {
            deps.fs
                .write(&path, &SITE_TEMPLATES.replace("{app}", &deps.app_name))?;
        }
        Ok(path)
    }

    /// Arranca MariaDB (o MySQL) si hace falta y crea la base de datos del
    /// sitio.
    async fn database_ready(
        &self,
        db: &str,
        progress: &Progress<'_>,
    ) -> Result<RenderContext, AppError> {
        let engine = self.database_service();
        if self.supervisor.state(engine) != ServiceState::Running {
            progress.step(format!("Iniciando {}", engine.label()));
            self.start(engine).await?;
        }
        if self.supervisor.state(engine) != ServiceState::Running {
            let detail = self
                .supervisor
                .snapshot()
                .into_iter()
                .find(|s| s.id == engine)
                .and_then(|s| s.detail)
                .unwrap_or_default();
            return Err(AppError::Process(format!(
                "{} no está en marcha y el sitio necesita una base de datos. {detail}",
                engine.label()
            )));
        }
        progress.step(format!("Creando la base de datos {db}"));
        let ctx = self.supervisor.context()?;
        mariadb::create_database(&ctx, db).await?;
        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_site_templates() {
        let list = parse_templates(SITE_TEMPLATES);
        let names: Vec<_> = list.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Symfony", "CodeIgniter 4", "CakePHP", "Drupal"]);
        assert_eq!(
            list[0].action,
            "composer create-project symfony/skeleton %s"
        );
        let list = parse_templates("A=x\na=y\n; nota\nB = https://x/y.zip \nsin-igual\n");
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].action, "https://x/y.zip");
    }

    #[test]
    fn tail_strips_colors_and_keeps_last_lines() {
        let text = "a\n\n\u{1b}[32mok\u{1b}[39m\n\u{1b}[1;31mError\u{1b}[0m: falló\n";
        assert_eq!(tail(text), "a\nok\nError: falló");
        let many: String = (0..20).map(|i| format!("l{i}\n")).collect();
        assert_eq!(tail(&many).lines().count(), OUTPUT_TAIL);
        assert!(tail(&many).ends_with("l19"));
    }
}
