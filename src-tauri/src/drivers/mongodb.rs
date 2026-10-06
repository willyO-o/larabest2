//! MongoDB: base de datos de documentos, solo en 127.0.0.1.

use std::path::PathBuf;

use async_trait::async_trait;
use minijinja::context;

use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

pub struct MongodbDriver;

#[async_trait]
impl ServiceDriver for MongodbDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Mongodb
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[]
    }

    fn default_port(&self) -> u16 {
        27017
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        vec![ctx.config.mongodb_port]
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![
            ctx.paths.logs().join("mongodb.log"),
            ctx.paths.logs().join("mongodb-console.log"),
        ]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        let install = ctx.install(ServiceId::Mongodb)?;
        let fs = ctx.fs.as_ref();
        let data = ctx.paths.data(ServiceId::Mongodb);
        fs.create_dir_all(&data)?;
        fs.create_dir_all(&ctx.paths.logs())?;
        let conf = templates::render(
            templates::MONGOD_CONF,
            context! {
                app_name => ctx.app_name,
                version => install.version,
                data => slash(&data),
                logs => slash(&ctx.paths.logs()),
                port => ctx.config.mongodb_port,
            },
        )?;
        write_if_changed(fs, &ctx.paths.mongodb_conf(), &conf)?;
        Ok(())
    }

    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        let (Ok(program), Ok(dir)) = (
            ctx.program(ServiceId::Mongodb, "mongod"),
            ctx.install_dir(ServiceId::Mongodb),
        ) else {
            return Vec::new();
        };
        vec![ProcessSpec::new(program, dir)
            .arg("--config")
            .arg(slash(&ctx.paths.mongodb_conf()))
            .log_to(ctx.paths.logs().join("mongodb-console.log"))]
    }

    /// MongoDB cierra en orden con SIGTERM o Ctrl+C. Forzarlo no daña los
    /// datos (tiene journal), pero obliga a recuperarse al arrancar.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        if pids.iter().any(|&pid| runner.interrupt(pid))
            && wait_for_exit(runner, pids, ctx.stop_grace).await
        {
            return Ok(());
        }
        force_stop(runner, pids).await
    }

    async fn health(&self, ctx: &RenderContext) -> Health {
        if ctx.ports.is_listening(ctx.config.mongodb_port) {
            Health::Up
        } else {
            Health::Down(format!(
                "sin respuesta en el puerto {}",
                ctx.config.mongodb_port
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::exe;
    use crate::drivers::test_support::fixture;

    #[tokio::test]
    async fn renders_yaml_config_and_starts_with_it() {
        let mut f = fixture();
        f.ctx.versions.insert(
            ServiceId::Mongodb,
            crate::domain::InstalledVersion {
                service: ServiceId::Mongodb,
                version: "8.0.13".into(),
                path: "C:/APP/bin/mongodb/8.0.13".into(),
            },
        );
        MongodbDriver.render_config(&f.ctx).await.unwrap();
        let conf = f.fs.read("C:/APP/etc/mongodb/mongod.conf");
        assert!(conf.contains("  dbPath: \"C:/APP/data/mongodb\"\n"));
        assert!(conf.contains("  path: \"C:/APP/logs/mongodb.log\"\n"));
        assert!(conf.contains("  bindIp: 127.0.0.1\n  port: 27017\n"));
        let spec = &MongodbDriver.start_specs(&f.ctx)[0];
        assert_eq!(
            slash(&spec.program),
            format!("C:/APP/bin/mongodb/8.0.13/bin/{}", exe("mongod"))
        );
        assert_eq!(
            spec.args,
            vec!["--config", "C:/APP/etc/mongodb/mongod.conf"]
        );
    }

    #[test]
    fn nothing_to_start_without_mongodb() {
        let f = fixture();
        assert!(MongodbDriver.start_specs(&f.ctx).is_empty());
    }
}
