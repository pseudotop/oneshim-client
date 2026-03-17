use anyhow::Result;
use directories::ProjectDirs;
use oneshim_api_contracts::integration::IntegrationOutboundRuntimeStatus;
use oneshim_core::config::AppConfig;
use oneshim_core::config_manager::ConfigManager;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::runtime::{Handle, Runtime};
use tracing::{info, warn};

use crate::bootstrap_preflight::BootstrapPreflightCoordinator;
#[cfg(feature = "server")]
use crate::server_runtime_context::ServerBootstrapContext;

pub(crate) struct BootstrapRuntimeBundle {
    pub(crate) db_path: PathBuf,
    pub(crate) data_dir_path: PathBuf,
    pub(crate) config_manager: ConfigManager,
    pub(crate) config: AppConfig,
    pub(crate) runtime_handle: Handle,
    pub(crate) web_port: Arc<AtomicU16>,
    #[cfg(feature = "server")]
    pub(crate) server: ServerBootstrapContext,
    #[cfg(not(feature = "server"))]
    pub(crate) integration_runtime_status: IntegrationOutboundRuntimeStatus,
}

impl BootstrapRuntimeBundle {
    pub(crate) fn frontend_web_port(&self) -> u16 {
        self.web_port.load(Ordering::Relaxed)
    }

    #[cfg(feature = "server")]
    pub(crate) fn integration_runtime_status(&self) -> IntegrationOutboundRuntimeStatus {
        self.server.integration_runtime_status()
    }

    #[cfg(not(feature = "server"))]
    pub(crate) fn integration_runtime_status(&self) -> IntegrationOutboundRuntimeStatus {
        self.integration_runtime_status.clone()
    }
}

pub(crate) struct BootstrapRuntimeBuilder {
    data_dir_override: Option<PathBuf>,
}

impl BootstrapRuntimeBuilder {
    pub(crate) fn new() -> Self {
        Self {
            data_dir_override: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_data_dir_override(mut self, data_dir_override: PathBuf) -> Self {
        self.data_dir_override = Some(data_dir_override);
        self
    }

    pub(crate) fn build(&self) -> Result<BootstrapRuntimeBundle> {
        let db_path = resolve_db_path(self.data_dir_override.as_deref());
        let data_dir_path = db_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&data_dir_path)?;
        info!("data directory: {}", data_dir_path.display());

        let config_manager = ConfigManager::new().unwrap_or_else(|error| {
            warn!("settings init failure, using defaults: {error}");
            let fallback_path = data_dir_path.join("config.json");
            ConfigManager::with_path(fallback_path).expect("failed to create config manager")
        });
        info!("settings file: {:?}", config_manager.config_path());

        let runtime_handle = spawn_background_runtime()?;
        let config = config_manager.get();
        let web_port = Arc::new(AtomicU16::new(config.web.port));
        BootstrapPreflightCoordinator::run(&config, &data_dir_path);

        #[cfg(feature = "server")]
        {
            let server = ServerBootstrapContext::build(&config, &data_dir_path)
                .map_err(|error| std::io::Error::other(error.to_string()))?;

            Ok(BootstrapRuntimeBundle {
                db_path,
                data_dir_path,
                config_manager,
                config,
                runtime_handle,
                web_port,
                server,
            })
        }

        #[cfg(not(feature = "server"))]
        {
            Ok(BootstrapRuntimeBundle {
                db_path,
                data_dir_path,
                config_manager,
                config,
                runtime_handle,
                web_port,
                integration_runtime_status: IntegrationOutboundRuntimeStatus::default(),
            })
        }
    }
}

fn resolve_db_path(data_dir: Option<&Path>) -> PathBuf {
    data_dir
        .map(|directory| directory.join("oneshim.db"))
        .or_else(|| {
            ProjectDirs::from("com", "oneshim", "agent")
                .map(|project| project.data_dir().join("oneshim.db"))
        })
        .unwrap_or_else(|| PathBuf::from("./oneshim.db"))
}

fn spawn_background_runtime() -> Result<Handle> {
    let runtime = Runtime::new()?;
    let handle = runtime.handle().clone();
    std::thread::spawn(move || {
        runtime.block_on(std::future::pending::<()>());
    });
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_db_path_default() {
        let path = resolve_db_path(None);
        assert!(path.to_string_lossy().contains("oneshim.db"));
    }

    #[test]
    fn resolve_db_path_custom() {
        let path = resolve_db_path(Some(Path::new("/tmp/test_data")));
        assert_eq!(path, PathBuf::from("/tmp/test_data/oneshim.db"));
    }

    #[test]
    fn builder_uses_override_directory_for_database_path() {
        let bundle = BootstrapRuntimeBuilder::new()
            .with_data_dir_override(PathBuf::from("/tmp/bootstrap_override"))
            .build()
            .expect("bootstrap builder should succeed");

        assert_eq!(
            bundle.db_path,
            PathBuf::from("/tmp/bootstrap_override/oneshim.db")
        );
    }
}
