//! Plantillas embebidas en el binario.

use minijinja::{Environment, Value};

use crate::error::AppError;

pub const NGINX_CONF: &str = "nginx.conf";
pub const VHOST_CONF: &str = "vhost.conf";
pub const PHP_INI: &str = "php.ini";
pub const MY_CNF: &str = "my.cnf";
pub const ADMINER_PHP: &str = "adminer.php";
pub const COMPOSER_BAT: &str = "composer.bat";

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    let templates = [
        (NGINX_CONF, include_str!("../../templates/nginx.conf")),
        (VHOST_CONF, include_str!("../../templates/vhost.conf")),
        (PHP_INI, include_str!("../../templates/php.ini")),
        (MY_CNF, include_str!("../../templates/my.cnf")),
        (ADMINER_PHP, include_str!("../../templates/adminer.php")),
        (COMPOSER_BAT, include_str!("../../templates/composer.bat")),
    ];
    for (name, source) in templates {
        env.add_template(name, source)
            .expect("las plantillas embebidas son válidas");
    }
    env
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
        ] {
            assert!(env.get_template(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn missing_variable_renders_empty_not_panics() {
        assert!(render(VHOST_CONF, context! {}).is_ok());
    }
}
