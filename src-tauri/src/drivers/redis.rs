//! Redis: caché, sesiones y colas (por ejemplo para Laravel).

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use minijinja::context;

use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

const CLI_TIMEOUT: Duration = Duration::from_secs(5);
/// `redis.conf` visto desde `data/redis`, la carpeta de trabajo.
const CONF_FROM_DATA: &str = "../../etc/redis/redis.conf";

pub struct RedisDriver;

/// `redis-cli` contra 127.0.0.1.
fn cli(ctx: &RenderContext, command: &str) -> Result<ProcessSpec, AppError> {
    let dir = ctx.install_dir(ServiceId::Redis)?;
    Ok(
        ProcessSpec::new(ctx.program(ServiceId::Redis, "redis-cli")?, &dir)
            .arg("-h")
            .arg("127.0.0.1")
            .arg("-p")
            .arg(ctx.config.redis_port.to_string())
            .arg(command),
    )
}

#[async_trait]
impl ServiceDriver for RedisDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Redis
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[]
    }

    fn default_port(&self) -> u16 {
        6379
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        vec![ctx.config.redis_port]
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![ctx.paths.logs().join("redis.log")]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let install = ctx.install(ServiceId::Redis)?;
        let fs = ctx.fs.as_ref();
        fs.create_dir_all(&ctx.paths.data(ServiceId::Redis))?;
        fs.create_dir_all(&ctx.paths.logs())?;
        templates::ensure_user_file(
            fs,
            &ctx.paths.redis_user_conf(),
            templates::USER_REDIS_CONF,
            &ctx.app_name,
        )?;
        let conf = templates::render(
            templates::REDIS_CONF,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                port => ctx.config.redis_port,
            },
        )?;
        write_if_changed(fs, &ctx.paths.redis_conf(), &conf)?;
        Ok(())
    }

    /// Desde `data/redis` y con rutas relativas: el Redis de Windows
    /// (compilado con MSYS2 o Cygwin) no entiende rutas con letra de unidad.
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        match ctx.program(ServiceId::Redis, "redis-server") {
            Ok(program) => vec![ProcessSpec::new(program, ctx.paths.data(ServiceId::Redis))
                .arg(CONF_FROM_DATA)
                .log_to(ctx.paths.logs().join("redis.log"))],
            Err(_) => Vec::new(),
        }
    }

    /// `redis-cli shutdown` guarda los datos y termina; si no responde, se
    /// le pide al sistema y, al vencer el plazo, se fuerza.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if let Ok(spec) = cli(ctx, "shutdown") {
            let _ = runner.run(&spec, CLI_TIMEOUT).await;
        }
        if wait_for_exit(runner, pids, ctx.stop_grace).await {
            return Ok(());
        }
        if pids.iter().any(|&pid| runner.interrupt(pid))
            && wait_for_exit(runner, pids, ctx.stop_grace).await
        {
            return Ok(());
        }
        force_stop(runner, pids).await
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        let spec = match cli(ctx, "ping") {
            Ok(spec) => spec,
            Err(e) => return Health::Down(e.to_string()),
        };
        match ctx.runner.run(&spec, CLI_TIMEOUT).await {
            Ok(out) if out.stdout.trim() == "PONG" => Health::Up,
            Ok(out) => Health::Down(out.text()),
            Err(e) => Health::Down(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::slash;
    use crate::drivers::exe;
    use crate::drivers::test_support::{fixture, homebrew_fixture, Fixture};
    use crate::ports::{CommandOutput, FileStore};
    use std::path::Path;

    fn with_redis(mut f: Fixture, path: &str) -> Fixture {
        f.ctx.versions.insert(
            ServiceId::Redis,
            crate::domain::InstalledVersion {
                service: ServiceId::Redis,
                version: "7.4.2".into(),
                path: path.into(),
            },
        );
        f
    }

    #[tokio::test]
    async fn renders_local_only_config_with_relative_paths() {
        let f = with_redis(fixture(), "C:/APP/bin/redis/7.4.2");
        RedisDriver.render_config(&f.ctx).await.unwrap();
        let conf = f.fs.read("C:/APP/etc/redis/redis.conf");
        assert!(conf.contains("bind 127.0.0.1\nport 6379\n"));
        assert!(conf.contains("dir ./\n"));
        assert!(!conf.contains("C:"), "{conf}");
        assert!(conf.ends_with("include ../../etc/redis/custom/user.conf\n"));
        assert!(f.fs.exists(Path::new("C:/APP/etc/redis/custom/user.conf")));
        assert!(f.fs.exists(Path::new("C:/APP/data/redis")));
        // Desde data/redis, la ruta relativa llega al redis.conf generado.
        let spec = &RedisDriver.start_specs(&f.ctx)[0];
        assert_eq!(slash(&spec.cwd), "C:/APP/data/redis");
        assert_eq!(spec.args, vec!["../../etc/redis/redis.conf"]);
        assert_eq!(
            slash(&spec.program),
            format!("C:/APP/bin/redis/7.4.2/{}", exe("redis-server"))
        );
    }

    #[tokio::test]
    async fn health_expects_pong() {
        let f = with_redis(fixture(), "C:/APP/bin/redis/7.4.2");
        f.runner.set_run_output(CommandOutput {
            code: Some(0),
            stdout: "PONG\n".into(),
            stderr: String::new(),
        });
        assert!(RedisDriver.health(&f.ctx).await.is_up());
        let ping = &f.runner.ran()[0];
        assert_eq!(ping.args, vec!["-h", "127.0.0.1", "-p", "6379", "ping"]);
        f.runner.set_run_output(CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "Could not connect".into(),
        });
        assert!(!RedisDriver.health(&f.ctx).await.is_up());
    }

    #[tokio::test]
    async fn stop_saves_with_shutdown_and_forces_only_at_the_end() {
        let f = with_redis(homebrew_fixture(), "/opt/homebrew/Cellar/redis/8.2.1");
        f.runner.set_alive(5, true);
        f.runner.kill_on_run(5);
        RedisDriver
            .stop(&f.ctx, &[5], f.runner.as_ref())
            .await
            .unwrap();
        let ran = f.runner.ran();
        assert_eq!(
            slash(&ran[0].program),
            "/opt/homebrew/Cellar/redis/8.2.1/bin/redis-cli"
        );
        assert_eq!(ran[0].args.last().unwrap(), "shutdown");
        assert!(f.runner.interrupted().is_empty());
        assert!(f.runner.stopped().is_empty());

        // Si no responde ni a Ctrl+C, se fuerza.
        f.runner.set_alive(6, true);
        f.runner.deaf(6);
        RedisDriver
            .stop(&f.ctx, &[6], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(f.runner.interrupted(), vec![6]);
        assert_eq!(f.runner.stopped(), vec![(6, Duration::ZERO)]);
    }
}
