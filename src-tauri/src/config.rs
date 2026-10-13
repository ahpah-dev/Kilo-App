use crate::models::AppConfig;
use std::path::PathBuf;
use std::sync::RwLock;

/// App settings backed by `config.json` in the app config directory.
pub struct ConfigStore {
    path: Option<PathBuf>,
    inner: RwLock<AppConfig>,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        let config = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<AppConfig>(&text).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            inner: RwLock::new(config),
        }
    }

    pub fn in_memory() -> Self {
        Self {
            path: None,
            inner: RwLock::new(AppConfig::default()),
        }
    }

    pub fn get(&self) -> AppConfig {
        self.inner.read().expect("config lock poisoned").clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut AppConfig)) {
        let snapshot = {
            let mut config = self.inner.write().expect("config lock poisoned");
            f(&mut config);
            config.clone()
        };
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match serde_json::to_string_pretty(&snapshot) {
                Ok(text) => {
                    if let Err(e) = std::fs::write(path, text) {
                        log::error!("failed to save config: {e}");
                    }
                }
                Err(e) => log::error!("failed to serialize config: {e}"),
            }
        }
    }
}
