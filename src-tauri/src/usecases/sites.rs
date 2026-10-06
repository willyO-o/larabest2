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
            let document_root = if fs.exists(&public) {
                public
            } else {
                dir.clone()
            };
            let domain = format!("{}.{}", name.to_ascii_lowercase(), cfg.tld);
            let url = site_url(cfg, &domain);
            Site {
                name,
                domain,
                url,
                path: slash(&dir),
                document_root: slash(&document_root),
                in_hosts: false,
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
}
