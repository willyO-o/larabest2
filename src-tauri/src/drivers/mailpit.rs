//! Mailpit: atrapa el correo que envían los sitios (SMTP) y lo muestra en
//! una página web. PHP y Laravel le envían todo.

use std::path::PathBuf;

use async_trait::async_trait;

use super::{force_stop, wait_for_exit, RenderContext, ServiceDriver};
use crate::domain::{slash, Health, ServiceId};
use crate::error::AppError;
use crate::ports::{ProcessRunner, ProcessSpec};

pub struct MailpitDriver;

/// Dirección de la bandeja de correo.
pub fn web_url(ctx_port: u16) -> String {
    format!("http://127.0.0.1:{ctx_port}/")
}

#[async_trait]
impl ServiceDriver for MailpitDriver {
    fn id(&self) -> ServiceId {
        ServiceId::Mailpit
    }

    fn depends_on(&self) -> &[ServiceId] {
        &[]
    }

    fn default_port(&self) -> u16 {
        1025
    }

    fn ports(&self, ctx: &RenderContext) -> Vec<u16> {
        vec![ctx.config.mailpit_smtp_port, ctx.config.mailpit_http_port]
    }

    fn log_files(&self, ctx: &RenderContext) -> Vec<PathBuf> {
        vec![ctx.paths.logs().join("mailpit.log")]
    }

    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError> {
        ctx.install(ServiceId::Mailpit)?;
        ctx.fs.create_dir_all(&ctx.paths.data(ServiceId::Mailpit))?;
        ctx.fs.create_dir_all(&ctx.paths.logs())?;
        Ok(())
    }

    /// Solo en 127.0.0.1. Acepta cualquier usuario y contraseña SMTP sin
    /// TLS, para que funcione con lo que traiga el `.env` del proyecto.
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec> {
        let Ok(program) = ctx.program(ServiceId::Mailpit, "mailpit") else {
            return Vec::new();
        };
        let data = ctx.paths.data(ServiceId::Mailpit);
        vec![ProcessSpec::new(program, &data)
            .arg("--listen")
            .arg(format!("127.0.0.1:{}", ctx.config.mailpit_http_port))
            .arg("--smtp")
            .arg(format!("127.0.0.1:{}", ctx.config.mailpit_smtp_port))
            .arg("--database")
            .arg(slash(&data.join("mailpit.db")))
            .arg("--smtp-auth-accept-any")
            .arg("--smtp-auth-allow-insecure")
            .log_to(ctx.paths.logs().join("mailpit.log"))]
    }

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
        let down: Vec<String> = self
            .ports(ctx)
            .into_iter()
            .filter(|&p| !ctx.ports.is_listening(p))
            .map(|p| p.to_string())
            .collect();
        if down.is_empty() {
            Health::Up
        } else {
            Health::Down(format!("sin respuesta en los puertos {}", down.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::test_support::homebrew_fixture;
    use std::time::Duration;

    fn ctx() -> crate::drivers::test_support::Fixture {
        let mut f = homebrew_fixture();
        f.ctx.versions.insert(
            ServiceId::Mailpit,
            crate::domain::InstalledVersion {
                service: ServiceId::Mailpit,
                version: "1.27.7".into(),
                path: "/opt/homebrew/Cellar/mailpit/1.27.7".into(),
            },
        );
        f
    }

    #[test]
    fn listens_only_locally_and_keeps_mail_in_data() {
        let f = ctx();
        let spec = &MailpitDriver.start_specs(&f.ctx)[0];
        assert_eq!(
            slash(&spec.program),
            "/opt/homebrew/Cellar/mailpit/1.27.7/bin/mailpit"
        );
        assert_eq!(
            spec.args[..6],
            [
                "--listen",
                "127.0.0.1:8025",
                "--smtp",
                "127.0.0.1:1025",
                "--database",
                "/Users/ana/LARABEST/data/mailpit/mailpit.db"
            ]
        );
        assert_eq!(MailpitDriver.ports(&f.ctx), vec![1025, 8025]);
    }

    #[tokio::test]
    async fn health_needs_smtp_and_web() {
        let f = ctx();
        f.ports.set_listening(&[1025]);
        assert_eq!(
            MailpitDriver.health(&f.ctx).await,
            Health::Down("sin respuesta en los puertos 8025".into())
        );
        f.ports.set_listening(&[1025, 8025]);
        assert!(MailpitDriver.health(&f.ctx).await.is_up());
    }

    #[tokio::test]
    async fn stop_asks_first_then_forces() {
        let f = ctx();
        f.runner.set_alive(3, true);
        MailpitDriver
            .stop(&f.ctx, &[3], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(f.runner.interrupted(), vec![3]);
        assert!(f.runner.stopped().is_empty());

        f.runner.set_alive(4, true);
        f.runner.deaf(4);
        MailpitDriver
            .stop(&f.ctx, &[4], f.runner.as_ref())
            .await
            .unwrap();
        assert_eq!(f.runner.stopped(), vec![(4, Duration::ZERO)]);
    }
}
