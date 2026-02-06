// SPDX-License-Identifier: EUPL-1.2-or-later
// Copyright © 2026-present ARI Contributors

//! Configuration management for IPCP instances
//!
//! Supports both command-line arguments and TOML configuration files.
//! Handles bootstrap vs. member IPCP modes with appropriate parameters.

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// IPCP operational mode
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IpcpMode {
    /// Bootstrap IPCP - first in the DIF, has static address
    Bootstrap,
    /// Member IPCP - enrolls with bootstrap to get address
    Member,
    /// Demo mode - runs the original demo without networking
    Demo,
}

impl std::fmt::Display for IpcpMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcpMode::Bootstrap => write!(f, "bootstrap"),
            IpcpMode::Member => write!(f, "member"),
            IpcpMode::Demo => write!(f, "demo"),
        }
    }
}

impl std::str::FromStr for IpcpMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bootstrap" => Ok(IpcpMode::Bootstrap),
            "member" => Ok(IpcpMode::Member),
            "demo" => Ok(IpcpMode::Demo),
            _ => Err(format!(
                "Invalid mode: {}. Use 'bootstrap', 'member', or 'demo'",
                s
            )),
        }
    }
}

/// Command-line arguments for IPCP
#[derive(Parser, Debug)]
#[command(name = "ari-ipcp")]
#[command(author = "ARI Contributors")]
#[command(version = "0.1.0")]
#[command(about = "RINA IPC Process", long_about = None)]
pub struct CliArgs {
    /// Path to TOML configuration file (overrides other arguments)
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// IPCP name
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    /// Operating mode: bootstrap, member, or demo
    #[arg(long, value_name = "MODE", default_value = "demo")]
    pub mode: IpcpMode,

    /// DIF name to join
    #[arg(long, value_name = "DIF")]
    pub dif_name: Option<String>,

    /// RINA address (required for bootstrap mode)
    #[arg(long, value_name = "ADDRESS")]
    pub address: Option<u64>,

    /// Address to bind UDP socket (e.g., "0.0.0.0:7000")
    #[arg(long, value_name = "ADDR:PORT")]
    pub bind: Option<String>,

    /// Bootstrap peer addresses for enrollment (member mode only)
    /// Format: "host:port" or "host:port,host:port"
    #[arg(long, value_name = "PEERS", value_delimiter = ',')]
    pub bootstrap_peers: Option<Vec<String>>,

    /// Address pool start (bootstrap mode only)
    #[arg(long, value_name = "ADDRESS", default_value = "1002")]
    pub address_pool_start: u64,

    /// Address pool end (bootstrap mode only)
    #[arg(long, value_name = "ADDRESS", default_value = "1999")]
    pub address_pool_end: u64,
}

/// Bootstrap peer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapPeer {
    /// Network address (host:port)
    pub address: String,
    /// Optional RINA address of the peer
    pub rina_addr: Option<u64>,
}

/// Static route configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticRoute {
    /// Destination RINA address
    pub destination: u64,
    /// Next hop network address (host:port)
    pub next_hop_address: String,
    /// Next hop RINA address
    pub next_hop_rina_addr: u64,
}

/// TOML configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlConfig {
    pub ipcp: IpcpConfig,
    pub dif: DifConfig,
    pub shim: ShimConfig,
    #[serde(default)]
    pub enrollment: EnrollmentConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub rib: RibConfig,
}

/// IPCP section of config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcpConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub ipcp_type: String,
    pub mode: IpcpMode,
}

/// DIF section of config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifConfig {
    pub name: String,
    /// Only for bootstrap mode
    pub address: Option<u64>,
    /// Address pool for bootstrap mode
    #[serde(default)]
    pub address_pool_start: Option<u64>,
    #[serde(default)]
    pub address_pool_end: Option<u64>,
}

/// Shim layer section of config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShimConfig {
    pub bind_address: String,
    pub bind_port: u16,
}

/// Enrollment section of config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentConfig {
    #[serde(default)]
    pub bootstrap_peers: Vec<BootstrapPeer>,
    /// Timeout for a single enrollment attempt (seconds)
    #[serde(default = "default_enrollment_timeout")]
    pub timeout_secs: u64,
    /// Maximum number of retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial backoff duration in milliseconds (doubles on each retry)
    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
}

fn default_enrollment_timeout() -> u64 {
    5
}

fn default_max_retries() -> u32 {
    3
}

fn default_initial_backoff_ms() -> u64 {
    1000
}

impl Default for EnrollmentConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: Vec::new(),
            timeout_secs: default_enrollment_timeout(),
            max_retries: default_max_retries(),
            initial_backoff_ms: default_initial_backoff_ms(),
        }
    }
}

/// Routing section of config
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    /// Static routes (bootstrap only)
    #[serde(default)]
    pub static_routes: Vec<StaticRoute>,
    /// Enable persistence of dynamic routes (save/load from snapshot file)
    #[serde(default)]
    pub enable_route_persistence: bool,
    /// Path to dynamic route snapshot file (TOML format)
    #[serde(default = "default_route_snapshot_path")]
    pub route_snapshot_path: String,
    /// Default TTL for dynamic routes in seconds (0 = never expires)
    #[serde(default = "default_route_ttl_seconds")]
    pub route_ttl_seconds: u64,
    /// Interval between automatic snapshots in seconds (0 = disabled)
    #[serde(default = "default_snapshot_interval_seconds")]
    pub route_snapshot_interval_seconds: u64,
}

fn default_route_snapshot_path() -> String {
    "dynamic-routes.toml".to_string()
}

fn default_route_ttl_seconds() -> u64 {
    3600 // 1 hour
}

fn default_snapshot_interval_seconds() -> u64 {
    300 // 5 minutes
}

/// RIB section of config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RibConfig {
    /// Enable persistence of RIB state (save/load from snapshot file)
    #[serde(default)]
    pub enable_rib_persistence: bool,
    /// Path to RIB snapshot file (bincode format)
    #[serde(default = "default_rib_snapshot_path")]
    pub rib_snapshot_path: String,
    /// Interval between automatic RIB snapshots in seconds (0 = disabled)
    #[serde(default = "default_rib_snapshot_interval_seconds")]
    pub rib_snapshot_interval_seconds: u64,
    /// Maximum number of changes to keep in change log for incremental sync
    #[serde(default = "default_change_log_size")]
    pub change_log_size: usize,
    /// RIB synchronization interval for members (seconds, 0 = disabled)
    #[serde(default = "default_rib_sync_interval_seconds")]
    pub rib_sync_interval_secs: u64,
}

fn default_rib_snapshot_path() -> String {
    "rib-snapshot.bin".to_string()
}

fn default_rib_snapshot_interval_seconds() -> u64 {
    300 // 5 minutes
}

fn default_change_log_size() -> usize {
    1000
}

fn default_rib_sync_interval_seconds() -> u64 {
    30 // 30 seconds
}

impl Default for RibConfig {
    fn default() -> Self {
        Self {
            enable_rib_persistence: false,
            rib_snapshot_path: default_rib_snapshot_path(),
            rib_snapshot_interval_seconds: default_rib_snapshot_interval_seconds(),
            change_log_size: default_change_log_size(),
            rib_sync_interval_secs: default_rib_sync_interval_seconds(),
        }
    }
}

/// Unified configuration after parsing CLI or file
#[derive(Debug, Clone)]
pub struct IpcpConfiguration {
    pub name: String,
    pub mode: IpcpMode,
    pub dif_name: String,
    pub address: Option<u64>,
    pub bind_address: String,
    pub bootstrap_peers: Vec<String>,
    pub address_pool_start: u64,
    pub address_pool_end: u64,
    pub enrollment_timeout_secs: u64,
    pub enrollment_max_retries: u32,
    pub enrollment_initial_backoff_ms: u64,
    pub static_routes: Vec<StaticRoute>,
    pub enable_route_persistence: bool,
    pub route_snapshot_path: String,
    pub route_ttl_seconds: u64,
    pub route_snapshot_interval_seconds: u64,
    pub enable_rib_persistence: bool,
    pub rib_snapshot_path: String,
    pub rib_snapshot_interval_seconds: u64,
    pub change_log_size: usize,
    pub rib_sync_interval_secs: u64,
}

impl IpcpConfiguration {
    /// Creates configuration from command-line arguments
    pub fn from_cli(args: CliArgs) -> Result<Self, String> {
        // If config file is specified, load from file
        if let Some(config_path) = args.config {
            return Self::from_file(&config_path);
        }

        // Otherwise, use CLI arguments
        let mode = args.mode;

        // Validate required fields based on mode
        match mode {
            IpcpMode::Demo => {
                // Demo mode doesn't need configuration
                Ok(Self {
                    name: args.name.unwrap_or_else(|| "demo-ipcp".to_string()),
                    mode: IpcpMode::Demo,
                    dif_name: "demo-dif".to_string(),
                    address: None,
                    bind_address: String::new(),
                    bootstrap_peers: vec![],
                    address_pool_start: 1002,
                    address_pool_end: 1999,
                    enrollment_timeout_secs: default_enrollment_timeout(),
                    enrollment_max_retries: default_max_retries(),
                    enrollment_initial_backoff_ms: default_initial_backoff_ms(),
                    static_routes: vec![],
                    enable_route_persistence: false,
                    route_snapshot_path: default_route_snapshot_path(),
                    route_ttl_seconds: default_route_ttl_seconds(),
                    route_snapshot_interval_seconds: default_snapshot_interval_seconds(),
                    enable_rib_persistence: false,
                    rib_snapshot_path: default_rib_snapshot_path(),
                    rib_snapshot_interval_seconds: default_rib_snapshot_interval_seconds(),
                    change_log_size: default_change_log_size(),
                    rib_sync_interval_secs: default_rib_sync_interval_seconds(),
                })
            }
            IpcpMode::Bootstrap => {
                let name = args.name.ok_or("--name is required for bootstrap mode")?;
                let dif_name = args
                    .dif_name
                    .ok_or("--dif-name is required for bootstrap mode")?;
                let address = args
                    .address
                    .ok_or("--address is required for bootstrap mode")?;
                let bind = args.bind.ok_or("--bind is required for bootstrap mode")?;

                Ok(Self {
                    name,
                    mode: IpcpMode::Bootstrap,
                    dif_name,
                    address: Some(address),
                    bind_address: bind,
                    bootstrap_peers: vec![],
                    address_pool_start: args.address_pool_start,
                    address_pool_end: args.address_pool_end,
                    enrollment_timeout_secs: default_enrollment_timeout(),
                    enrollment_max_retries: default_max_retries(),
                    enrollment_initial_backoff_ms: default_initial_backoff_ms(),
                    static_routes: vec![], // No CLI support for routes yet
                    enable_route_persistence: false,
                    route_snapshot_path: default_route_snapshot_path(),
                    route_ttl_seconds: default_route_ttl_seconds(),
                    route_snapshot_interval_seconds: default_snapshot_interval_seconds(),
                    enable_rib_persistence: false,
                    rib_snapshot_path: default_rib_snapshot_path(),
                    rib_snapshot_interval_seconds: default_rib_snapshot_interval_seconds(),
                    change_log_size: default_change_log_size(),
                    rib_sync_interval_secs: default_rib_sync_interval_seconds(),
                })
            }
            IpcpMode::Member => {
                let name = args.name.ok_or("--name is required for member mode")?;
                let dif_name = args
                    .dif_name
                    .ok_or("--dif-name is required for member mode")?;
                let bind = args.bind.ok_or("--bind is required for member mode")?;
                let peers = args
                    .bootstrap_peers
                    .ok_or("--bootstrap-peers is required for member mode")?;

                Ok(Self {
                    name,
                    mode: IpcpMode::Member,
                    dif_name,
                    address: None, // Will be assigned during enrollment
                    bind_address: bind,
                    bootstrap_peers: peers,
                    address_pool_start: args.address_pool_start,
                    address_pool_end: args.address_pool_end,
                    enrollment_timeout_secs: default_enrollment_timeout(),
                    enrollment_max_retries: default_max_retries(),
                    enrollment_initial_backoff_ms: default_initial_backoff_ms(),
                    static_routes: vec![], // Members learn routes from bootstrap
                    enable_route_persistence: false,
                    route_snapshot_path: default_route_snapshot_path(),
                    route_ttl_seconds: default_route_ttl_seconds(),
                    route_snapshot_interval_seconds: default_snapshot_interval_seconds(),
                    enable_rib_persistence: false,
                    rib_snapshot_path: default_rib_snapshot_path(),
                    rib_snapshot_interval_seconds: default_rib_snapshot_interval_seconds(),
                    change_log_size: default_change_log_size(),
                    rib_sync_interval_secs: default_rib_sync_interval_seconds(),
                })
            }
        }
    }

    /// Loads configuration from a TOML file
    pub fn from_file(path: &PathBuf) -> Result<Self, String> {
        let contents =
            fs::read_to_string(path).map_err(|e| format!("Failed to read config file: {}", e))?;

        let config: TomlConfig =
            toml::from_str(&contents).map_err(|e| format!("Failed to parse TOML config: {}", e))?;

        let bind_address = format!("{}:{}", config.shim.bind_address, config.shim.bind_port);

        let bootstrap_peers = config
            .enrollment
            .bootstrap_peers
            .iter()
            .map(|peer| peer.address.clone())
            .collect();

        Ok(Self {
            name: config.ipcp.name,
            mode: config.ipcp.mode,
            dif_name: config.dif.name,
            address: config.dif.address,
            bind_address,
            bootstrap_peers,
            address_pool_start: config.dif.address_pool_start.unwrap_or(1002),
            address_pool_end: config.dif.address_pool_end.unwrap_or(1999),
            enrollment_timeout_secs: config.enrollment.timeout_secs,
            enrollment_max_retries: config.enrollment.max_retries,
            enrollment_initial_backoff_ms: config.enrollment.initial_backoff_ms,
            static_routes: config.routing.static_routes,
            enable_route_persistence: config.routing.enable_route_persistence,
            route_snapshot_path: config.routing.route_snapshot_path,
            route_ttl_seconds: config.routing.route_ttl_seconds,
            route_snapshot_interval_seconds: config.routing.route_snapshot_interval_seconds,
            enable_rib_persistence: config.rib.enable_rib_persistence,
            rib_snapshot_path: config.rib.rib_snapshot_path,
            rib_snapshot_interval_seconds: config.rib.rib_snapshot_interval_seconds,
            change_log_size: config.rib.change_log_size,
            rib_sync_interval_secs: config.rib.rib_sync_interval_secs,
        })
    }

    /// Validates configuration based on mode
    pub fn validate(&self) -> Result<(), String> {
        match self.mode {
            IpcpMode::Bootstrap => {
                if self.address.is_none() {
                    return Err("Bootstrap mode requires an address".to_string());
                }
                if self.bind_address.is_empty() {
                    return Err("Bootstrap mode requires a bind address".to_string());
                }
            }
            IpcpMode::Member => {
                if self.bootstrap_peers.is_empty() {
                    return Err("Member mode requires at least one bootstrap peer".to_string());
                }
                if self.bind_address.is_empty() {
                    return Err("Member mode requires a bind address".to_string());
                }
            }
            IpcpMode::Demo => {
                // Demo mode has minimal requirements
            }
        }
        Ok(())
    }

    /// Prints configuration summary
    pub fn print_summary(&self) {
        println!("=== IPCP Configuration ===");
        println!("Name: {}", self.name);
        println!("Mode: {}", self.mode);
        println!("DIF: {}", self.dif_name);

        if let Some(addr) = self.address {
            println!("RINA Address: {}", addr);
        }

        if !self.bind_address.is_empty() {
            println!("Bind Address: {}", self.bind_address);
        }

        if !self.bootstrap_peers.is_empty() {
            println!("Bootstrap Peers: {:?}", self.bootstrap_peers);
        }

        if self.mode == IpcpMode::Bootstrap {
            println!(
                "Address Pool: {}-{}",
                self.address_pool_start, self.address_pool_end
            );
        }

        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipcp_mode_parsing() {
        assert_eq!(
            "bootstrap".parse::<IpcpMode>().unwrap(),
            IpcpMode::Bootstrap
        );
        assert_eq!("member".parse::<IpcpMode>().unwrap(), IpcpMode::Member);
        assert_eq!("demo".parse::<IpcpMode>().unwrap(), IpcpMode::Demo);
        assert!("invalid".parse::<IpcpMode>().is_err());
    }
}
