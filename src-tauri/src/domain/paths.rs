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

    /// Carpeta de phpMyAdmin (el zip descomprimido, con `index.php`).
    pub fn bin_phpmyadmin(&self) -> PathBuf {
        self.bin().join("phpmyadmin")
    }

    pub fn phpmyadmin_index(&self) -> PathBuf {
        self.bin_phpmyadmin().join("index.php")
    }

    /// phpMyAdmin solo lee `config.inc.php` de su propia carpeta: la app
    /// lo genera ahí y deja los ajustes propios en `etc/phpmyadmin/`.
    pub fn phpmyadmin_config(&self) -> PathBuf {
        self.bin_phpmyadmin().join("config.inc.php")
    }

    pub fn phpmyadmin_custom(&self) -> PathBuf {
        self.etc().join("phpmyadmin").join("custom.inc.php")
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

    pub fn apache_conf(&self) -> PathBuf {
        self.etc_service(ServiceId::Apache).join("httpd.conf")
    }

    pub fn apache_sites(&self) -> PathBuf {
        self.etc_service(ServiceId::Apache).join("sites")
    }

    /// Inclusiones del usuario por sitio (`<sitio>.conf`); nunca se
    /// regeneran.
    pub fn apache_custom(&self) -> PathBuf {
        self.etc_service(ServiceId::Apache).join("custom")
    }

    pub fn etc_php(&self) -> PathBuf {
        self.etc().join("php")
    }

    pub fn php_ini(&self) -> PathBuf {
        self.etc_php().join("php.ini")
    }

    /// Configuración de `php-fpm` (macOS).
    pub fn php_fpm_conf(&self) -> PathBuf {
        self.etc_php().join("php-fpm.conf")
    }

    /// Archivos `.ini` propios del usuario; PHP los carga con
    /// `PHP_INI_SCAN_DIR` y nunca se regeneran.
    pub fn php_custom(&self) -> PathBuf {
        self.etc_php().join("custom")
    }

    pub fn php_user_ini(&self) -> PathBuf {
        self.php_custom().join("user.ini")
    }

    pub fn etc_mariadb(&self) -> PathBuf {
        self.etc().join("mariadb")
    }

    pub fn mariadb_cnf(&self) -> PathBuf {
        self.etc_mariadb().join("my.cnf")
    }

    /// Archivos `.cnf` propios del usuario (`!includedir`), nunca se regeneran.
    pub fn mariadb_custom(&self) -> PathBuf {
        self.etc_mariadb().join("custom")
    }

    pub fn mariadb_user_cnf(&self) -> PathBuf {
        self.mariadb_custom().join("user.cnf")
    }

    /// Socket de MariaDB en macOS: propio, para no chocar con otro MySQL
    /// que use `/tmp/mysql.sock`.
    pub fn mariadb_socket(&self) -> PathBuf {
        self.tmp().join("mariadb.sock")
    }

    /// `my.cnf` generado de MariaDB o MySQL (`etc/<motor>/my.cnf`).
    pub fn db_cnf(&self, id: ServiceId) -> PathBuf {
        self.etc_service(id).join("my.cnf")
    }

    /// Ajustes propios de MariaDB o MySQL (`!includedir`).
    pub fn db_custom(&self, id: ServiceId) -> PathBuf {
        self.etc_service(id).join("custom")
    }

    pub fn db_user_cnf(&self, id: ServiceId) -> PathBuf {
        self.db_custom(id).join("user.cnf")
    }

    /// Socket propio en macOS (`tmp/mariadb.sock` o `tmp/mysql.sock`).
    pub fn db_socket(&self, id: ServiceId) -> PathBuf {
        self.tmp().join(format!("{}.sock", id.key()))
    }

    /// Respaldos de las bases de datos: una carpeta por fecha.
    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }

    /// Procesos propios que arrancan con "Iniciar todo" (como en Laragon).
    pub fn procfile(&self) -> PathBuf {
        self.root.join("Procfile")
    }

    pub fn procfile_pids(&self) -> PathBuf {
        self.tmp().join("procfile.pids")
    }

    /// Plantillas propias de "Crear sitio".
    pub fn site_templates(&self) -> PathBuf {
        self.etc().join("sitios.conf")
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

    pub fn site_cert(&self, domain: &str) -> PathBuf {
        self.etc_ssl().join(format!("{domain}.crt"))
    }

    pub fn site_key(&self, domain: &str) -> PathBuf {
        self.etc_ssl().join(format!("{domain}.key"))
    }

    /// Descargas reutilizables (WordPress).
    pub fn downloads(&self) -> PathBuf {
        self.tmp().join("downloads")
    }

    pub fn composer_phar(&self) -> PathBuf {
        self.bin_composer().join("composer.phar")
    }

    pub fn data_mariadb(&self) -> PathBuf {
        self.data(ServiceId::Mariadb)
    }

    /// Datos de un servicio: `data/<servicio>`.
    pub fn data(&self, id: ServiceId) -> PathBuf {
        self.root.join("data").join(id.key())
    }

    /// Configuración generada de un servicio: `etc/<servicio>`.
    pub fn etc_service(&self, id: ServiceId) -> PathBuf {
        self.etc().join(id.key())
    }

    pub fn redis_conf(&self) -> PathBuf {
        self.etc_service(ServiceId::Redis).join("redis.conf")
    }

    /// Ajustes propios de Redis (incluidos al final de `redis.conf`).
    pub fn redis_user_conf(&self) -> PathBuf {
        self.etc_service(ServiceId::Redis)
            .join("custom")
            .join("user.conf")
    }

    pub fn mongodb_conf(&self) -> PathBuf {
        self.etc_service(ServiceId::Mongodb).join("mongod.conf")
    }

    /// `postgresql.conf` lo crea `initdb` en los datos y es del usuario: la
    /// app solo fija puerto y direcciones por línea de comandos.
    pub fn postgresql_conf(&self) -> PathBuf {
        self.data(ServiceId::Postgresql).join("postgresql.conf")
    }

    pub fn postgresql_hba(&self) -> PathBuf {
        self.data(ServiceId::Postgresql).join("pg_hba.conf")
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
        let mut dirs: Vec<PathBuf> = ServiceId::ALL
            .iter()
            .map(|&id| self.bin_service(id))
            .collect();
        dirs.extend([
            self.bin_adminer(),
            self.bin_composer(),
            self.www(),
            self.nginx_sites(),
            self.nginx_custom(),
            self.php_custom(),
            self.mariadb_custom(),
            self.db_custom(ServiceId::Mysql),
            self.redis_user_conf()
                .parent()
                .expect("custom/")
                .to_path_buf(),
            self.etc_ssl(),
            self.root.join("data"),
            self.logs(),
            self.tmp(),
        ]);
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
        assert_eq!(
            slash(&p.db_cnf(ServiceId::Mysql)),
            "C:/APP/etc/mysql/my.cnf"
        );
        assert_eq!(
            slash(&p.db_cnf(ServiceId::Mariadb)),
            slash(&p.mariadb_cnf())
        );
        assert_eq!(
            slash(&p.db_socket(ServiceId::Mysql)),
            "C:/APP/tmp/mysql.sock"
        );
        assert_eq!(
            slash(&p.db_socket(ServiceId::Mariadb)),
            slash(&p.mariadb_socket())
        );
    }
}
