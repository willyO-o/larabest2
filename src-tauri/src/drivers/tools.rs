//! Herramientas web que la app sirve como sitios propios: Adminer
//! (`adminer.<tld>`) y phpMyAdmin (`phpmyadmin.<tld>`). Sirve igual para
//! cualquier servidor web.

use std::path::PathBuf;

use minijinja::context;
use sha2::{Digest, Sha256};

use super::{templates, RenderContext};
use crate::domain::{slash, Paths, Site};
use crate::error::AppError;
use crate::ports::{write_if_changed, FileStore};

pub const ADMINER: &str = "adminer";
pub const PHPMYADMIN: &str = "phpmyadmin";
pub const ALL: [&str; 2] = [ADMINER, PHPMYADMIN];

/// `true` si la herramienta está copiada en `bin/`.
pub fn installed(fs: &dyn FileStore, paths: &Paths, name: &str) -> bool {
    match name {
        ADMINER => fs.exists(&paths.adminer_php()),
        PHPMYADMIN => fs.exists(&paths.phpmyadmin_index()),
        _ => false,
    }
}

/// Nombres reservados: un sitio nuevo no puede llamarse así.
pub fn is_reserved(name: &str) -> bool {
    ALL.iter().any(|t| t.eq_ignore_ascii_case(name))
}

/// Las instaladas que no tapa un proyecto de `www/` con el mismo nombre.
pub fn served(ctx: &RenderContext, sites: &[Site]) -> Vec<&'static str> {
    ALL.into_iter()
        .filter(|&name| installed(ctx.fs.as_ref(), &ctx.paths, name))
        .filter(|name| !sites.iter().any(|s| s.name.eq_ignore_ascii_case(name)))
        .collect()
}

/// Secreto estable para las cookies de phpMyAdmin (32 caracteres), propio
/// de cada carpeta raíz. Con `auth_type = config` no protege nada, pero
/// phpMyAdmin avisa si falta.
fn blowfish_secret(paths: &Paths, app_name: &str) -> String {
    let digest = Sha256::digest(format!("{app_name} phpMyAdmin {}", slash(paths.root())));
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// Escribe lo que la herramienta necesita y devuelve su carpeta pública y
/// si cambió algún archivo.
pub fn prepare(ctx: &RenderContext, name: &str) -> Result<(PathBuf, bool), AppError> {
    let fs = ctx.fs.as_ref();
    match name {
        ADMINER => {
            let wrapper = templates::render(
                templates::ADMINER_PHP,
                context! {
                    app_name => ctx.app_name,
                    adminer_php => slash(&ctx.paths.adminer_php()),
                },
            )?;
            let index = ctx.paths.etc_adminer().join("index.php");
            let changed = write_if_changed(fs, &index, &wrapper)?;
            Ok((ctx.paths.etc_adminer(), changed))
        }
        _ => {
            let tmp = ctx.paths.tmp().join("phpmyadmin");
            fs.create_dir_all(&tmp)?;
            templates::ensure_user_file(
                fs,
                &ctx.paths.phpmyadmin_custom(),
                templates::PHPMYADMIN_CUSTOM,
                &ctx.app_name,
            )?;
            let config = templates::render(
                templates::PHPMYADMIN_CONFIG,
                context! {
                    app_name => ctx.app_name,
                    secret => blowfish_secret(&ctx.paths, &ctx.app_name),
                    port => ctx.config.mariadb_port,
                    tmp => slash(&tmp),
                    custom => slash(&ctx.paths.phpmyadmin_custom()),
                },
            )?;
            let changed = write_if_changed(fs, &ctx.paths.phpmyadmin_config(), &config)?;
            Ok((ctx.paths.bin_phpmyadmin(), changed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::test_support::fixture;
    use std::path::Path;

    #[test]
    fn phpmyadmin_logs_in_as_root_with_a_stable_secret() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/bin/phpmyadmin/index.php"), "<?php")
            .unwrap();
        assert_eq!(served(&f.ctx, &[]), vec![PHPMYADMIN]);
        let (root, changed) = prepare(&f.ctx, PHPMYADMIN).unwrap();
        assert_eq!(slash(&root), "C:/APP/bin/phpmyadmin");
        assert!(changed);
        let config = f.fs.read("C:/APP/bin/phpmyadmin/config.inc.php");
        assert!(config.contains("$cfg['Servers'][$i]['auth_type'] = 'config';"));
        assert!(config.contains("$cfg['Servers'][$i]['port'] = '3306';"));
        assert!(config.contains("$cfg['Servers'][$i]['AllowNoPassword'] = true;"));
        assert!(config.contains("$cfg['TempDir'] = 'C:/APP/tmp/phpmyadmin';"));
        let secret = config
            .split("['blowfish_secret'] = '")
            .nth(1)
            .and_then(|rest| rest.split('\'').next())
            .unwrap();
        assert_eq!(secret.len(), 32);
        // Sin cambios no reescribe, y el secreto no cambia.
        assert!(!prepare(&f.ctx, PHPMYADMIN).unwrap().1);
        assert!(f
            .fs
            .read("C:/APP/etc/phpmyadmin/custom.inc.php")
            .contains("NO se regenera"));
    }

    #[test]
    fn a_project_with_the_same_name_wins() {
        let f = fixture();
        f.fs.write(Path::new("C:/APP/bin/adminer/adminer.php"), "<?php")
            .unwrap();
        let project = Site {
            name: "adminer".into(),
            domain: "adminer.test".into(),
            url: String::new(),
            path: String::new(),
            document_root: String::new(),
            in_hosts: false,
        };
        assert!(served(&f.ctx, &[project]).is_empty());
        assert!(is_reserved("phpMyAdmin"));
        assert!(!is_reserved("blog"));
    }
}
