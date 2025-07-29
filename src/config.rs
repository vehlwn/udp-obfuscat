use anyhow::Context;

/// UDP proxy with a simple xor cipher obfuscation
#[derive(clap::Parser)]
#[command(version, about, long_about)]
pub struct Cli {
    /// Read options from a config file
    #[arg(short, long, value_name = "FILE")]
    config_file: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct GeneralOptions {
    /// Switch to this user when running as root after binding a socket to drop privileges
    pub user: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ListenerOptions {
    /// Array of hosts and ports where to bind listening client or server UDP socket. Can be either
    /// ip address or domain name (127.0.0.1:5000, [::]:5000 localhost:5000)
    pub address: Vec<String>,

    /// Resolve listening address to IPv4 only
    #[serde(default = "default_listen_ipv4_only")]
    pub ipv4_only: bool,

    /// Resolve listening address to IPv6 only
    #[serde(default = "default_listen_ipv6_only")]
    pub ipv6_only: bool,
}
fn default_listen_ipv4_only() -> bool {
    false
}
fn default_listen_ipv6_only() -> bool {
    false
}

#[derive(Debug, serde::Deserialize)]
pub struct RemoteOptions {
    /// Address of an udp-obfuscat server in client mode or UDP upstream in server mode
    pub address: String,

    /// Resolve upstream address to IPv4 only
    #[serde(default = "default_upstream_ipv4_only")]
    pub ipv4_only: bool,

    /// Resolve upstream address to IPv6 only
    #[serde(default = "default_upstream_ipv6_only")]
    pub ipv6_only: bool,
}

fn default_upstream_ipv4_only() -> bool {
    false
}
fn default_upstream_ipv6_only() -> bool {
    false
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct LoggingOptions {
    /// Off, Error, Warn, Info, Debug, Trace,
    pub log_level: Option<log::LevelFilter>,

    /// use systemd-journal instead of env_logger
    #[serde(default)]
    pub journald: JournaldOption,

    /// env_logger only: disable timestamps in log messages
    #[serde(default)]
    pub disable_timestamps: DisableTimestamps,
}

#[derive(Debug, Copy, Clone, serde::Deserialize)]
pub struct JournaldOption(bool);
impl Default for JournaldOption {
    fn default() -> Self {
        return Self(false);
    }
}
impl Into<bool> for JournaldOption {
    fn into(self) -> bool {
        self.0
    }
}

#[derive(Debug, Copy, Clone, serde::Deserialize)]
pub struct DisableTimestamps(bool);
impl Default for DisableTimestamps {
    fn default() -> Self {
        return Self(true);
    }
}
impl Into<bool> for DisableTimestamps {
    fn into(self) -> bool {
        self.0
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FilterOptions {
    /// Base64-encoded key for a Xor filter
    pub xor_key: String,

    /// Apply filter to only first head_len bytes of each packet
    pub head_len: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralOptions,
    pub listener: ListenerOptions,
    pub remote: RemoteOptions,
    #[serde(default)]
    pub logging: LoggingOptions,
    pub filters: FilterOptions,
}

pub fn parse_config() -> anyhow::Result<Config> {
    use clap::Parser;
    let cli = Cli::parse();
    let content = std::fs::read_to_string(&cli.config_file)
        .with_context(|| format!("Failed to read config file '{}'", cli.config_file))?;
    let toml_config: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse toml config from '{}'", cli.config_file))?;
    return Ok(toml_config);
}
