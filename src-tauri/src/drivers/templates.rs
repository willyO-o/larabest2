//! Plantillas embebidas en el binario.

use minijinja::{Environment, Value};

use crate::error::AppError;

pub const NGINX_CONF: &str = "nginx.conf";
pub const VHOST_CONF: &str = "vhost.conf";
pub const HTTPD_CONF: &str = "httpd.conf";
pub const HTTPD_VHOST_CONF: &str = "httpd-vhost.conf";
pub const PHP_INI: &str = "php.ini";
pub const MY_CNF: &str = "my.cnf";
pub const MYSQL_CNF: &str = "mysql.cnf";
pub const SHARE_NGINX: &str = "share-nginx.conf";
pub const SHARE_HTTPD: &str = "share-httpd.conf";
pub const USER_MYSQL_CNF: &str = "user-mysql.cnf";
pub const ADMINER_PHP: &str = "adminer.php";
pub const COMPOSER_BAT: &str = "composer.bat";
pub const COMPOSER_SH: &str = "composer.sh";
pub const PHP_FPM_CONF: &str = "php-fpm.conf";
pub const MIME_TYPES: &str = "mime.types";
pub const FASTCGI_PARAMS: &str = "fastcgi_params";
pub const REDIS_CONF: &str = "redis.conf";
pub const USER_REDIS_CONF: &str = "user-redis.conf";
pub const MONGOD_CONF: &str = "mongod.conf";
pub const PHPMYADMIN_CONFIG: &str = "phpmyadmin.config.inc.php";
pub const PHPMYADMIN_CUSTOM: &str = "phpmyadmin.custom.inc.php";
pub const USER_INI: &str = "user.ini";
pub const USER_CNF: &str = "user.cnf";
pub const WP_CONFIG: &str = "wp-config.php";
pub const BLANK_INDEX: &str = "blank-index.php";

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    let templates = [
        (NGINX_CONF, include_str!("../../templates/nginx.conf")),
        (VHOST_CONF, include_str!("../../templates/vhost.conf")),
        (HTTPD_CONF, include_str!("../../templates/httpd.conf")),
        (
            HTTPD_VHOST_CONF,
            include_str!("../../templates/httpd-vhost.conf"),
        ),
        (PHP_INI, include_str!("../../templates/php.ini")),
        (MY_CNF, include_str!("../../templates/my.cnf")),
        (MYSQL_CNF, include_str!("../../templates/mysql.cnf")),
        (
            SHARE_NGINX,
            include_str!("../../templates/share-nginx.conf"),
        ),
        (
            SHARE_HTTPD,
            include_str!("../../templates/share-httpd.conf"),
        ),
        (
            USER_MYSQL_CNF,
            include_str!("../../templates/user-mysql.cnf"),
        ),
        (ADMINER_PHP, include_str!("../../templates/adminer.php")),
        (COMPOSER_BAT, include_str!("../../templates/composer.bat")),
        (COMPOSER_SH, include_str!("../../templates/composer.sh")),
        (PHP_FPM_CONF, include_str!("../../templates/php-fpm.conf")),
        (MIME_TYPES, include_str!("../../templates/mime.types")),
        (
            FASTCGI_PARAMS,
            include_str!("../../templates/fastcgi_params"),
        ),
        (REDIS_CONF, include_str!("../../templates/redis.conf")),
        (
            USER_REDIS_CONF,
            include_str!("../../templates/user-redis.conf"),
        ),
        (MONGOD_CONF, include_str!("../../templates/mongod.conf")),
        (
            PHPMYADMIN_CONFIG,
            include_str!("../../templates/phpmyadmin.config.inc.php"),
        ),
        (
            PHPMYADMIN_CUSTOM,
            include_str!("../../templates/phpmyadmin.custom.inc.php"),
        ),
        (USER_INI, include_str!("../../templates/user.ini")),
        (USER_CNF, include_str!("../../templates/user.cnf")),
        (WP_CONFIG, include_str!("../../templates/wp-config.php")),
        (BLANK_INDEX, include_str!("../../templates/blank-index.php")),
    ];
    for (name, source) in templates {
        env.add_template(name, source)
            .expect("las plantillas embebidas son válidas");
    }
    env
}

/// Crea un archivo de ajustes propios desde su plantilla solo si no existe.
pub fn ensure_user_file(
    fs: &dyn crate::ports::FileStore,
    path: &std::path::Path,
    template: &str,
    app_name: &str,
) -> Result<(), AppError> {
    if fs.exists(path) {
        return Ok(());
    }
    let content = render(template, minijinja::context! { app_name => app_name })?;
    fs.write(path, &content)
}

pub fn render(name: &str, ctx: Value) -> Result<String, AppError> {
    let env = environment();
    let template = env.get_template(name).map_err(|e| AppError::Template {
        name: name.into(),
        detail: e.to_string(),
    })?;
    template.render(ctx).map_err(|e| AppError::Template {
        name: name.into(),
        detail: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    #[test]
    fn all_templates_compile() {
        let env = environment();
        for name in [
            NGINX_CONF,
            VHOST_CONF,
            PHP_INI,
            MY_CNF,
            ADMINER_PHP,
            COMPOSER_BAT,
            COMPOSER_SH,
            PHP_FPM_CONF,
            MIME_TYPES,
            FASTCGI_PARAMS,
            REDIS_CONF,
            USER_REDIS_CONF,
            MONGOD_CONF,
            PHPMYADMIN_CONFIG,
            PHPMYADMIN_CUSTOM,
            USER_INI,
            USER_CNF,
            WP_CONFIG,
            BLANK_INDEX,
        ] {
            assert!(env.get_template(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn missing_variable_renders_empty_not_panics() {
        assert!(render(VHOST_CONF, context! {}).is_ok());
    }
}
