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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub key_strategy: LoadBalanceStrategy,
    #[serde(default)]
    pub provider_strategy: LoadBalanceStrategy,
    pub providers: Vec<ProviderConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            key_strategy: LoadBalanceStrategy::default(),
            provider_strategy: LoadBalanceStrategy::default(),
            providers: Vec::new(),
        }
    }
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            version: default_version(),
            log_level: default_log_level(),
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
    /// Validate the provider configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationError(
                "Provider name cannot be empty".to_string(),
            ));
        }
        if self.api_keys.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "Provider '{}' has no API keys configured",
                self.name
            )));
        }
        Ok(())
    }
}

/// Provider-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSettings {
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            timeout_seconds: default_timeout(),
            max_results: default_max_results(),
        }
    }
}

// Default value functions.
fn default_name() -> String {
    "websearch_load_balance".to_string()
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
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
    /// Load configuration from a YAML file.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        let content = std::fs::read_to_string(&path)?;
        let config: Config = serde_yaml::from_str(&content)?;

        // Validate all providers.
        for provider in &config.providers {
            provider.validate()?;
        }

        Ok(config)
    }

    /// Get enabled providers.
    pub fn enabled_providers(&self) -> Vec<&ProviderConfig> {
        self.providers.iter().filter(|p| p.enabled).collect()
    }

    /// Get all provider names.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.iter().map(|p| p.name.as_str()).collect()
    }
}