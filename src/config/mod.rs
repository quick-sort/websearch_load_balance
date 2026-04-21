//! Configuration module for loading YAML config.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse YAML: {0}")]
    ParseError(#[from] serde_yaml::Error),
    #[error("Config validation error: {0}")]
    ValidationError(String),
}

/// Main configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub key_strategy: LoadBalanceStrategy,
    #[serde(default)]
    pub provider_strategy: LoadBalanceStrategy,
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub paper_providers: Vec<PaperProviderConfig>,
}

/// Server metadata configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// HTTP server configuration
    #[serde(default)]
    pub http: Option<HttpConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            version: default_version(),
            log_level: default_log_level(),
            http: None,
        }
    }
}

/// HTTP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Enable HTTP server
    #[serde(default)]
    pub enabled: bool,
    /// Listen address
    #[serde(default = "default_host")]
    pub host: String,
    /// Listen port
    #[serde(default = "default_port")]
    pub port: u16,
    /// API key for authentication
    #[serde(default)]
    pub api_key: Option<String>,
    /// Path for MCP endpoint
    #[serde(default)]
    pub mcp_path: Option<String>,
    /// Allowed Host header values (for DNS rebinding protection).
    /// Defaults to loopback hosts. Set to allow Docker service names, domains, etc.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_host(),
            port: default_port(),
            api_key: None,
            mcp_path: Some("/mcp".to_string()),
            allowed_hosts: Vec::new(),
        }
    }
}

/// Load balancing strategy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalanceStrategy {
    #[serde(default)]
    pub r#type: StrategyType,
    #[serde(default = "default_fallback")]
    pub fallback: bool,
}

impl Default for LoadBalanceStrategy {
    fn default() -> Self {
        Self {
            r#type: StrategyType::default(),
            fallback: default_fallback(),
        }
    }
}

/// Strategy type for load balancing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyType {
    #[default]
    RoundRobin,
    Random,
}

/// Provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    pub api_keys: Vec<String>,
    #[serde(default)]
    pub settings: ProviderSettings,
}

impl ProviderConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationError(
                "Provider name cannot be empty".to_string(),
            ));
        }
        if self.api_keys.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "Provider {} has no API keys configured",
                self.name
            )));
        }
        Ok(())
    }
}

/// Paper provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperProviderConfig {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_keys: Vec<String>,
}

/// Provider-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSettings {
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
    #[serde(default)]
    pub api_variant: Option<String>,
}

impl ProviderSettings {
    pub fn variant(&self) -> &str {
        self.api_variant.as_deref().unwrap_or("standard")
    }
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            timeout_seconds: default_timeout(),
            max_results: default_max_results(),
            api_variant: None,
        }
    }
}

// Default value functions.
fn default_name() -> String {
    "one_search".to_string()
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_enabled() -> bool {
    true
}

fn default_fallback() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

fn default_max_results() -> u32 {
    10
}

impl Config {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        let content = std::fs::read_to_string(&path)?;
        let config: Config = serde_yaml::from_str(&content)?;

        for provider in &config.providers {
            provider.validate()?;
        }

        Ok(config)
    }

    pub fn enabled_providers(&self) -> Vec<&ProviderConfig> {
        self.providers.iter().filter(|p| p.enabled).collect()
    }

    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.iter().map(|p| p.name.as_str()).collect()
    }
}
