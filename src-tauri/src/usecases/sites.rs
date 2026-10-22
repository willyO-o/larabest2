use crate::domain::{is_valid_site_name, slash, AppConfig, Paths, Site};
use crate::ports::FileStore;

/// URL de un dominio: `https://` si HTTPS está activado.
pub fn site_url(cfg: &AppConfig, domain: &str) -> String {
    match (
        cfg.ssl,
        cfg.ssl && cfg.https_port == 443,
        cfg.http_port == 80,
    ) {
        (true, true, _) => format!("https://{domain}/"),
        (true, false, _) => format!("https://{domain}:{}/", cfg.https_port),
        (false, _, true) => format!("http://{domain}/"),
        (false, _, false) => format!("http://{domain}:{}/", cfg.http_port),
    }
}

/// Sin hosts virtuales: la carpeta dentro de `http://localhost/` (y su
/// `public/` si la tiene, como Laravel). Siempre por HTTP: el servidor de
/// `localhost` no tiene certificado.
pub fn localhost_url(cfg: &AppConfig, name: &str, public: bool) -> String {
    let host = if cfg.http_port == 80 {
        "localhost".to_string()
    } else {
        format!("localhost:{}", cfg.http_port)
    };
    let tail = if public { "public/" } else { "" };
    format!("http://{host}/{name}/{tail}")
}

/// URL de un proyecto de `www/`: su dominio `<carpeta>.<tld>` o, sin hosts
/// virtuales, su carpeta en `localhost`.
pub fn project_url(cfg: &AppConfig, name: &str, public: bool) -> String {
    if cfg.auto_vhosts {
        site_url(cfg, &format!("{}.{}", name.to_ascii_lowercase(), cfg.tld))
    } else {
        localhost_url(cfg, name, public)
    }
}

/// Cada subcarpeta válida de `www/` es un sitio. Las que no sirven como
/// dominio (espacios, puntos, guiones bajos) se ignoran.
pub fn scan_sites(fs: &dyn FileStore, paths: &Paths, cfg: &AppConfig) -> Vec<Site> {
    let www = paths.www();
    let mut names = fs.list_dirs(&www).unwrap_or_default();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names
        .into_iter()
        .filter(|n| is_valid_site_name(n))
        .map(|name| {
            let dir = www.join(&name);
            let public = dir.join("public");
            let has_public = fs.exists(&public);
            let document_root = if has_public { public } else { dir.clone() };
            let domain = format!("{}.{}", name.to_ascii_lowercase(), cfg.tld);
            let url = project_url(cfg, &name, has_public);
            Site {
                name,
                domain,
                url,
                path: slash(&dir),
                document_root: slash(&document_root),
                // Sin hosts virtuales no hace falta el archivo hosts.
                in_hosts: !cfg.auto_vhosts,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FileStore;
    use crate::testing::FakeFs;
    use std::path::Path;

    #[test]
    fn uses_public_folder_when_present() {
        let fs = FakeFs::default();
        fs.create_dir_all(Path::new("C:/APP/www/laravel/public"))
            .unwrap();
        fs.create_dir_all(Path::new("C:/APP/www/WordPress"))
            .unwrap();
        fs.create_dir_all(Path::new("C:/APP/www/mal nombre"))
            .unwrap();
        fs.write(Path::new("C:/APP/www/nota.txt"), "x").unwrap();

        let sites = scan_sites(&fs, &Paths::new("C:/APP"), &AppConfig::default());
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].name, "laravel");
        assert_eq!(sites[0].document_root, "C:/APP/www/laravel/public");
        assert_eq!(sites[1].domain, "wordpress.test");
        assert_eq!(sites[1].url, "http://wordpress.test/");
        assert_eq!(sites[1].document_root, "C:/APP/www/WordPress");
    }

    #[test]
    fn https_urls_when_ssl_is_enabled() {
        let mut cfg = AppConfig {
            ssl: true,
            ..AppConfig::default()
        };
        assert_eq!(site_url(&cfg, "blog.test"), "https://blog.test/");
        cfg.https_port = 8443;
        assert_eq!(site_url(&cfg, "blog.test"), "https://blog.test:8443/");
    }

    #[test]
    fn url_includes_non_default_port() {
        let fs = FakeFs::default();
        fs.create_dir_all(Path::new("C:/APP/www/blog")).unwrap();
        let cfg = AppConfig {
            http_port: 8080,
            ..AppConfig::default()
        };
        let sites = scan_sites(&fs, &Paths::new("C:/APP"), &cfg);
        assert_eq!(sites[0].url, "http://blog.test:8080/");
    }

    #[test]
    fn without_vhosts_sites_open_under_localhost() {
        let fs = FakeFs::default();
        fs.create_dir_all(Path::new("C:/APP/www/laravel/public"))
            .unwrap();
        fs.create_dir_all(Path::new("C:/APP/www/Blog")).unwrap();
        let mut cfg = AppConfig {
            auto_vhosts: false,
            ssl: true,
            ..AppConfig::default()
        };
        let sites = scan_sites(&fs, &Paths::new("C:/APP"), &cfg);
        assert_eq!(sites[0].url, "http://localhost/Blog/");
        assert_eq!(sites[1].url, "http://localhost/laravel/public/");
        assert!(sites.iter().all(|s| s.in_hosts));

        cfg.http_port = 8080;
        assert_eq!(
            project_url(&cfg, "blog", false),
            "http://localhost:8080/blog/"
        );
        cfg.auto_vhosts = true;
        cfg.tld = "lab".into();
        cfg.ssl = false;
        assert_eq!(project_url(&cfg, "Blog", false), "http://blog.lab:8080/");
    }
}
