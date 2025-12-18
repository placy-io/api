// Copyright 2025 Placy
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configuration management for placy-server.
//!
//! Uses the `config` crate to load settings from multiple sources:
//! - Default values
//! - Configuration files (config.toml, config.yaml, etc.)
//! - Environment variables (prefixed with PLACY_)

use config::{Config, ConfigError, Environment, File};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::path::PathBuf;

/// Main application settings.
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    /// Server configuration
    pub server: ServerSettings,
    /// Processing configuration
    pub processing: ProcessingSettings,
    /// Authentication configuration
    pub auth: AuthSettings,
    /// Observability configuration
    pub observability: ObservabilitySettings,
}

/// HTTP server settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerSettings {
    /// Host address to bind to
    #[serde(default = "default_host")]
    pub host: String,
    /// Port to listen on
    #[serde(default = "default_port")]
    pub port: u16,
    /// Number of worker threads (0 = auto)
    #[serde(default)]
    pub workers: usize,
    /// Request timeout in seconds
    #[serde(default = "default_request_timeout")]
    #[allow(dead_code)] // Reserved for future timeout configuration
    pub request_timeout_secs: u64,
    /// Keep-alive timeout in seconds
    #[serde(default = "default_keepalive")]
    #[allow(dead_code)] // Reserved for future keepalive configuration
    pub keepalive_secs: u64,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_request_timeout() -> u64 {
    300 // 5 minutes for large file processing
}

fn default_keepalive() -> u64 {
    75
}

/// File processing settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessingSettings {
    /// Maximum upload size in bytes
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: usize,
    /// Maximum individual file size in bytes
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    /// Maximum files in archive
    #[serde(default = "default_max_file_count")]
    pub max_file_count: usize,
    /// Maximum ZIP archive size in bytes
    #[serde(default = "default_max_zip_size")]
    pub max_zip_size: u64,
    /// Maximum compression ratio for zip bomb detection
    #[serde(default = "default_max_compression_ratio")]
    pub max_compression_ratio: f64,
    /// Temporary directory for processing
    #[serde(default = "default_temp_dir")]
    #[allow(dead_code)] // Reserved for future temp file handling
    pub temp_dir: PathBuf,
    /// Delete process.txt from output
    #[serde(default = "default_true")]
    pub delete_process_file: bool,
    /// Delete ignore.txt from output
    #[serde(default = "default_true")]
    pub delete_ignore_file: bool,
}

fn default_max_upload_size() -> usize {
    100 * 1024 * 1024 // 100 MB
}

fn default_max_file_size() -> u64 {
    10 * 1024 * 1024 // 10 MB
}

fn default_max_file_count() -> usize {
    1000
}

fn default_max_zip_size() -> u64 {
    100 * 1024 * 1024 // 100 MB
}

fn default_max_compression_ratio() -> f64 {
    100.0
}

fn default_temp_dir() -> PathBuf {
    std::env::temp_dir().join("placy")
}

fn default_true() -> bool {
    true
}

/// Authentication settings.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthSettings {
    /// Whether authentication is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// API keys for authentication (comma-separated in env var)
    #[serde(default)]
    pub api_keys: Vec<ApiKey>,
    /// Header name for API key
    #[serde(default = "default_api_key_header")]
    pub api_key_header: String,
    /// Whether metrics endpoint requires authentication
    #[serde(default = "default_true")]
    pub metrics_auth_enabled: bool,
}

fn default_api_key_header() -> String {
    "X-API-Key".to_string()
}

/// API key configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiKey {
    /// Name/identifier for the key
    pub name: String,
    /// The secret key value
    #[serde(deserialize_with = "deserialize_secret")]
    pub key: SecretString,
    /// Optional rate limit (requests per minute)
    pub rate_limit: Option<u32>,
}

fn deserialize_secret<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SecretString::from(s))
}

/// Observability settings (logging, metrics, tracing).
#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilitySettings {
    /// Logging configuration
    pub logging: LoggingSettings,
    /// Metrics configuration
    pub metrics: MetricsSettings,
    /// OpenTelemetry configuration
    pub otel: OtelSettings,
}

/// Logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingSettings {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log format (json, pretty, compact)
    #[serde(default = "default_log_format")]
    pub format: LogFormat,
    /// Enable console logging
    #[serde(default = "default_true")]
    pub console_enabled: bool,
    /// Enable file logging
    #[serde(default)]
    pub file_enabled: bool,
    /// Log file directory
    #[serde(default = "default_log_dir")]
    pub file_dir: PathBuf,
    /// Log file name prefix
    #[serde(default = "default_log_prefix")]
    pub file_prefix: String,
    /// Log rotation (daily, hourly, minutely)
    #[serde(default = "default_rotation")]
    pub file_rotation: LogRotation,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> LogFormat {
    LogFormat::Pretty
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("./logs")
}

fn default_log_prefix() -> String {
    "placy-server".to_string()
}

fn default_rotation() -> LogRotation {
    LogRotation::Daily
}

/// Log output format.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// JSON format for structured logging
    Json,
    /// Human-readable pretty format
    #[default]
    Pretty,
    /// Compact single-line format
    Compact,
}

/// Log file rotation strategy.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogRotation {
    /// Rotate daily
    #[default]
    Daily,
    /// Rotate hourly
    Hourly,
    /// Rotate every minute (for testing)
    Minutely,
    /// Never rotate
    Never,
}

/// Prometheus metrics configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsSettings {
    /// Enable Prometheus metrics
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Metrics endpoint path
    #[serde(default = "default_metrics_path")]
    pub path: String,
    /// Include default process metrics
    #[serde(default = "default_true")]
    #[allow(dead_code)] // Reserved for future process metrics configuration
    pub include_process_metrics: bool,
}

fn default_metrics_path() -> String {
    "/metrics".to_string()
}

/// OpenTelemetry configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct OtelSettings {
    /// Enable OpenTelemetry export
    #[serde(default)]
    pub enabled: bool,
    /// OTLP endpoint URL
    #[serde(default = "default_otel_endpoint")]
    pub endpoint: String,
    /// Service name for traces
    #[serde(default = "default_service_name")]
    pub service_name: String,
}

fn default_otel_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_service_name() -> String {
    "placy-server".to_string()
}

impl Settings {
    /// Load settings from multiple sources.
    ///
    /// Priority (highest to lowest):
    /// 1. Environment variables (PLACY_*)
    /// 2. Configuration file (config.toml/yaml/json)
    /// 3. Default values
    pub fn load() -> Result<Self, ConfigError> {
        let config_dir = std::env::var("PLACY_CONFIG_DIR").unwrap_or_else(|_| ".".to_string());

        let builder = Config::builder()
            // Start with defaults
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", 0)?
            .set_default("server.request_timeout_secs", 300)?
            .set_default("server.keepalive_secs", 75)?
            .set_default("processing.max_upload_size", 104857600)?
            .set_default("processing.max_file_size", 10485760)?
            .set_default("processing.max_file_count", 1000)?
            .set_default("processing.max_zip_size", 104857600)?
            .set_default("processing.max_compression_ratio", 100.0)?
            .set_default("processing.delete_process_file", true)?
            .set_default("processing.delete_ignore_file", true)?
            .set_default("auth.enabled", true)?
            .set_default("auth.api_key_header", "X-API-Key")?
            .set_default("auth.metrics_auth_enabled", true)?
            .set_default("observability.logging.level", "info")?
            .set_default("observability.logging.format", "pretty")?
            .set_default("observability.logging.console_enabled", true)?
            .set_default("observability.logging.file_enabled", false)?
            .set_default("observability.logging.file_prefix", "placy-server")?
            .set_default("observability.logging.file_rotation", "daily")?
            .set_default("observability.metrics.enabled", true)?
            .set_default("observability.metrics.path", "/metrics")?
            .set_default("observability.metrics.include_process_metrics", true)?
            .set_default("observability.otel.enabled", false)?
            .set_default("observability.otel.endpoint", "http://localhost:4317")?
            .set_default("observability.otel.service_name", "placy-server")?
            .set_default("observability.otel.traces_enabled", true)?
            .set_default("observability.otel.metrics_enabled", false)?
            .set_default("observability.otel.logs_enabled", false)?
            // Load from config file if exists
            .add_source(File::with_name(&format!("{config_dir}/config")).required(false))
            // Override with environment variables
            .add_source(
                Environment::with_prefix("PLACY")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
            );

        builder.build()?.try_deserialize()
    }

    /// Get server bind address.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

impl AuthSettings {
    /// Validate an API key against configured keys.
    pub fn validate_key(&self, key: &str) -> Option<&ApiKey> {
        if !self.enabled {
            return None;
        }

        self.api_keys.iter().find(|api_key| {
            constant_time_eq::constant_time_eq(
                key.as_bytes(),
                api_key.key.expose_secret().as_bytes(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Global lock to ensure tests run serially when modifying environment
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_settings() {
        let _guard = ENV_LOCK.lock().unwrap();

        // Clear any env vars that might interfere
        unsafe {
            std::env::remove_var("PLACY_SERVER__PORT");
            std::env::remove_var("PLACY_AUTH__ENABLED");
        }

        let settings = Settings::load().unwrap();
        assert_eq!(settings.server.port, 8080);
        assert_eq!(settings.server.host, "0.0.0.0");
        assert!(settings.auth.enabled);
    }

    #[test]
    fn test_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();

        unsafe {
            std::env::set_var("PLACY_SERVER__PORT", "9090");
            std::env::set_var("PLACY_AUTH__ENABLED", "false");
        }

        let settings = Settings::load().unwrap();
        assert_eq!(settings.server.port, 9090);
        assert!(!settings.auth.enabled);

        // Cleanup
        unsafe {
            std::env::remove_var("PLACY_SERVER__PORT");
            std::env::remove_var("PLACY_AUTH__ENABLED");
        }
    }
}
