use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use crate::error::{PylonError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub id: String,
    pub source_model: String,
    pub target_model: String,
    pub upstream: String,
    pub api_key: String,
    #[serde(default)]
    pub options: ProxyOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyOptions {
    pub default_temperature: Option<f32>,
    pub default_top_p: Option<f32>,
    pub default_top_k: Option<u32>,
    pub default_max_tokens: Option<u32>,
    #[serde(default = "default_true")]
    pub support_streaming: bool,
    #[serde(default)]
    pub support_tools: bool,
    #[serde(default)]
    pub support_vision: bool,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default)]
    pub extra_body: serde_json::Value,
}

fn default_true() -> bool { true }

impl Default for ProxyOptions {
    fn default() -> Self {
        Self {
            default_temperature: None,
            default_top_p: None,
            default_top_k: None,
            default_max_tokens: None,
            support_streaming: true,
            support_tools: false,
            support_vision: false,
            extra_headers: HashMap::new(),
            extra_body: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub proxies: HashMap<String, ProxyConfig>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            proxies: HashMap::new(),
        }
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
    config: RwLock<ConfigFile>,
}

impl ConfigManager {
    pub fn new() -> Self {
        let config_path = Self::get_config_path();
        let config = Self::load_config(&config_path).unwrap_or_default();
        
        Self {
            config_path,
            config: RwLock::new(config),
        }
    }

    fn get_config_path() -> PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".pylon").join("proxies.json")
        } else {
            PathBuf::from("/root/.pylon/proxies.json")
        }
    }

    fn load_config(path: &PathBuf) -> Result<ConfigFile> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            return Ok(ConfigFile::default());
        }
        
        let content = std::fs::read_to_string(path)?;
        let config: ConfigFile = serde_json::from_str(&content)?;
        Ok(config)
    }

    fn save_config(&self, config: &ConfigFile) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(config)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub async fn list(&self) -> Vec<ProxyConfig> {
        let config = self.config.read().await;
        config.proxies.values().cloned().collect()
    }

    pub async fn get(&self, source_model: &str) -> Option<ProxyConfig> {
        let config = self.config.read().await;
        config.proxies.get(source_model).cloned()
    }

    pub async fn add(&self, proxy: ProxyConfig) -> Result<()> {
        let mut config = self.config.write().await;
        config.proxies.insert(proxy.source_model.clone(), proxy.clone());
        self.save_config(&config)?;
        Ok(())
    }

    pub async fn update(&self, source_model: &str, proxy: ProxyConfig) -> Result<()> {
        let mut config = self.config.write().await;
        if !config.proxies.contains_key(source_model) {
            return Err(PylonError::ProxyNotFound(source_model.to_string()));
        }
        config.proxies.insert(proxy.source_model.clone(), proxy);
        self.save_config(&config)?;
        Ok(())
    }

    pub async fn delete(&self, source_model: &str) -> Result<()> {
        let mut config = self.config.write().await;
        if config.proxies.remove(source_model).is_none() {
            return Err(PylonError::ProxyNotFound(source_model.to_string()));
        }
        self.save_config(&config)?;
        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        let new_config = Self::load_config(&self.config_path)?;
        let mut config = self.config.write().await;
        *config = new_config;
        Ok(())
    }

    pub async fn list_models(&self) -> Vec<String> {
        let config = self.config.read().await;
        config.proxies.keys().cloned().collect()
    }
}

impl ProxyConfig {
    pub fn transform_request(&self, mut body: serde_json::Value) -> serde_json::Value {
        if let Some(obj) = body.as_object_mut() {
            if let Some(model) = obj.get("model") {
                if model.as_str() == Some(&self.source_model) {
                    obj.insert("model".to_string(), serde_json::Value::String(self.target_model.clone()));
                }
            }

            if !obj.contains_key("max_tokens") {
                if let Some(tokens) = self.options.default_max_tokens {
                    obj.insert("max_tokens".to_string(), serde_json::Value::Number(
                        serde_json::Number::from(tokens)
                    ));
                }
            }
        }
        body
    }
}