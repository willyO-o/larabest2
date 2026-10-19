//! MongoDB: base de datos de documentos, solo en 127.0.0.1.

use std::path::PathBuf;

use async_trait::async_trait;
use minijinja::context;

use super::{force_stop, templates, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{write_if_changed, ProcessRunner, ProcessSpec};

pub struct MongodbDriver;

/// Mensaje `OP_MSG` con `{shutdown: 1, $db: "admin"}`: apaga MongoDB en
/// orden. Desde 127.0.0.1 y sin usuarios no pide autenticación.
pub fn shutdown_message() -> Vec<u8> {
    let mut doc = Vec::new();
    doc.push(0x10); // int32
    doc.extend_from_slice(b"shutdown\0");
    doc.extend_from_slice(&1i32.to_le_bytes());
    doc.push(0x02); // string
    doc.extend_from_slice(b"$db\0");
    doc.extend_from_slice(&6i32.to_le_bytes());
    doc.extend_from_slice(b"admin\0");
    doc.push(0x00);
    let mut bson = ((doc.len() + 4) as i32).to_le_bytes().to_vec();
    bson.extend(doc);

    let mut body = 0u32.to_le_bytes().to_vec(); // flagBits
    body.push(0); // sección de tipo 0: el documento
    body.extend(bson);
    let mut msg = ((body.len() + 16) as i32).to_le_bytes().to_vec();
    msg.extend_from_slice(&1i32.to_le_bytes()); // requestID
    msg.extend_from_slice(&0i32.to_le_bytes()); // responseTo
    msg.extend_from_slice(&2013i32.to_le_bytes()); // OP_MSG
    msg.extend(body);
    msg
}

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

    /// Primero el comando `shutdown` por su puerto (en Windows el Ctrl+C no
    /// siempre le llega); si no termina, SIGTERM o Ctrl+C. Forzarlo no daña
    /// los datos (tiene journal), pero obliga a recuperarse al arrancar.
    async fn stop(
        &self,
        ctx: &RenderContext,
        pids: &[u32],
        runner: &dyn ProcessRunner,
    ) -> Result<(), AppError> {
        let ports = ctx.ports.clone();
        let port = ctx.config.mongodb_port;
        let sent = tokio::task::spawn_blocking(move || ports.send_tcp(port, &shutdown_message()))
            .await
            .unwrap_or(false);
        if sent && wait_for_exit(runner, pids, ctx.stop_grace).await {
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

    #[test]
    fn shutdown_is_a_well_formed_op_msg() {
        let msg = shutdown_message();
        let len = i32::from_le_bytes(msg[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, msg.len());
        assert_eq!(i32::from_le_bytes(msg[12..16].try_into().unwrap()), 2013);
        let doc = &msg[21..];
        let doc_len = i32::from_le_bytes(doc[0..4].try_into().unwrap()) as usize;
        assert_eq!(doc_len, doc.len());
        assert_eq!(*doc.last().unwrap(), 0);
        assert!(doc.windows(9).any(|w| w == b"shutdown\0"));
        assert!(doc.windows(6).any(|w| w == b"admin\0"));
    }
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
