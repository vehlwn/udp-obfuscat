use std::net::SocketAddr;

use anyhow::Context;

/// UDP proxy with a simple xor cipher obfuscation
#[derive(clap::Parser)]
#[command(version, about, long_about)]
pub struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config_file: Option<String>,

    /// Where to bind listening client or server UDP socket
    #[arg(short, long)]
    local_address: Option<SocketAddr>,

    /// Address of an udp-obfuscat server in client mode or UDP upstream in server mode
    #[arg(short, long)]
    remote_address: Option<SocketAddr>,

    /// Base64-encoded key for a Xor filter
    #[arg(long)]
    xor_key: Option<String>,

    /// Disable timestamps in log messages
    #[arg(long)]
    disable_timestamps: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct Config {
    pub user: Option<String>,
    pub log_level: Option<log::LevelFilter>,
    pub journald: bool,
    pub disable_timestamps: bool,
    pub local_address: SocketAddr,
    pub remote_address: SocketAddr,
    pub xor_key: String,
}

fn apply_cli_opts(config: &mut Config, cli: &Cli) {
    if let Some(local_address) = cli.local_address {
        config.local_address = local_address;
    }
    if let Some(remote_address) = cli.remote_address {
        config.remote_address = remote_address;
    }
    if let Some(ref xor_key) = cli.xor_key {
        config.xor_key = xor_key.clone();
    }
    if cli.disable_timestamps {
        config.disable_timestamps = true;
    }
}

pub fn parse_config() -> anyhow::Result<Config> {
    use clap::Parser;

    let cli = Cli::parse();
    if let Some(ref config_path) = cli.config_file {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file '{config_path}'"))?;
        let mut toml_config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse toml config from '{config_path}'"))?;
        apply_cli_opts(&mut toml_config, &cli);
        return Ok(toml_config);
    }
    return Ok(Config {
        user: None,
        log_level: None,
        journald: false,
        disable_timestamps: cli.disable_timestamps,
        local_address: cli.local_address.context("local_address is not set")?,
        remote_address: cli.remote_address.context("remote_address is not set")?,
        xor_key: cli.xor_key.context("xor_key is not set")?,
    });
}
