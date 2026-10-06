use std::path::{Path, PathBuf};

use super::ServiceId;

/// Rutas de la carpeta de datos. Todo cuelga de una sola raíz portable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Paths { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn bin_service(&self, id: ServiceId) -> PathBuf {
        self.bin().join(id.key())
    }

    /// Carpeta donde se copia `adminer.php`.
    pub fn bin_adminer(&self) -> PathBuf {
        self.bin().join("adminer")
    }

    pub fn adminer_php(&self) -> PathBuf {
        self.bin_adminer().join("adminer.php")
    }

    /// Carpeta donde se copia `composer.phar` (opcional).
    pub fn bin_composer(&self) -> PathBuf {
        self.bin().join("composer")
    }

    pub fn www(&self) -> PathBuf {
        self.root.join("www")
    }

    pub fn etc(&self) -> PathBuf {
        self.root.join("etc")
    }

    pub fn etc_nginx(&self) -> PathBuf {
        self.etc().join("nginx")
    }

    pub fn nginx_conf(&self) -> PathBuf {
        self.etc_nginx().join("nginx.conf")
    }

    pub fn nginx_sites(&self) -> PathBuf {
        self.etc_nginx().join("sites")
    }

    pub fn nginx_custom(&self) -> PathBuf {
        self.etc_nginx().join("custom")
    }

    pub fn etc_php(&self) -> PathBuf {
        self.etc().join("php")
    }

    pub fn php_ini(&self) -> PathBuf {
        self.etc_php().join("php.ini")
    }

    pub fn etc_mariadb(&self) -> PathBuf {
        self.etc().join("mariadb")
    }

    pub fn mariadb_cnf(&self) -> PathBuf {
        self.etc_mariadb().join("my.cnf")
    }

    /// Envoltorio generado que carga Adminer sin pedir contraseña.
    pub fn etc_adminer(&self) -> PathBuf {
        self.etc().join("adminer")
    }

    /// Atajos generados para la terminal (por ejemplo `composer.bat`).
    pub fn etc_shell(&self) -> PathBuf {
        self.etc().join("shell")
    }

    pub fn etc_ssl(&self) -> PathBuf {
        self.etc().join("ssl")
    }

    pub fn data_mariadb(&self) -> PathBuf {
        self.root.join("data").join("mariadb")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn pid_file(&self, id: ServiceId) -> PathBuf {
        self.tmp().join(format!("{}.pids", id.key()))
    }

    pub fn app_json(&self) -> PathBuf {
        self.root.join("app.json")
    }

    /// Carpetas que deben existir siempre. `data/mariadb` no está: la crea
    /// `mariadb-install-db`, que exige que no exista o esté vacía.
    pub fn layout_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![
            self.bin_service(ServiceId::Nginx),
            self.bin_service(ServiceId::Php),
            self.bin_service(ServiceId::Mariadb),
            self.bin_adminer(),
            self.bin_composer(),
            self.www(),
            self.nginx_sites(),
            self.nginx_custom(),
            self.etc_php(),
            self.etc_mariadb(),
            self.etc_ssl(),
            self.root.join("data"),
            self.logs(),
            self.tmp(),
        ];
        dirs.sort();
        dirs
    }
}

/// Ruta con barras `/`, que Nginx, PHP y MariaDB aceptan también en Windows.
pub fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_converts_backslashes() {
        assert_eq!(slash(Path::new(r"C:\APP\etc\nginx")), "C:/APP/etc/nginx");
    }

    #[test]
    fn derives_paths_from_root() {
        let p = Paths::new("C:/APP");
        assert_eq!(slash(&p.nginx_conf()), "C:/APP/etc/nginx/nginx.conf");
        assert_eq!(slash(&p.data_mariadb()), "C:/APP/data/mariadb");
        assert_eq!(slash(&p.pid_file(ServiceId::Php)), "C:/APP/tmp/php.pids");
        assert!(!p.layout_dirs().contains(&p.data_mariadb()));
    }
}
