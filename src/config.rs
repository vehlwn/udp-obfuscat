use std::net::SocketAddr;

use anyhow::Context;

#[derive(Debug)]
pub struct Config {
    pub user: Option<String>,
    pub log_level: Option<String>,
    pub disable_timestamps: bool,
    pub local_address: SocketAddr,
    pub remote_address: SocketAddr,
    pub xor_key: String,
}

pub fn parse_config() -> anyhow::Result<Config> {
    let cli = clap::command!()
        .arg(
            clap::Arg::new("config_path")
                .long("config-path")
                .short('c')
                .help("Alternative toml config file"),
        )
        .arg(
            clap::Arg::new("disable_timestamps")
                .long("disable-timestamps")
                .help("Disable timestamps in log messages. By default they are enabled")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("local_address")
                .long("local-address")
                .short('l')
                .value_parser(clap::value_parser!(SocketAddr))
                .help("Where to bind listening client UDP socket"),
        )
        .arg(
            clap::Arg::new("remote_address")
                .long("remote-address")
                .short('r')
                .value_parser(clap::value_parser!(SocketAddr))
                .help("Address of an udp-obfuscat server in client mode or UDP upstream in server mode"),
        )
        .arg(
            clap::Arg::new("xor_key")
                .long("xor-key")
                .help("Base64-encoded key for a Xor filter"),
        )
        .get_matches();

    let mut user = None;
    let mut log_level = None;
    let mut disable_timestamps = cli.get_flag("disable_timestamps");
    let mut local_address = if cli.contains_id("local_address") {
        cli.get_one::<SocketAddr>("local_address")
            .map(|x| x.clone())
    } else {
        None
    };
    let mut remote_address = if cli.contains_id("remote_address") {
        cli.get_one::<SocketAddr>("remote_address")
            .map(|x| x.clone())
    } else {
        None
    };
    let mut xor_key = cli.get_one::<String>("xor_key").map(|x| x.clone());

    if let Some(path) = cli.get_one::<String>("config_path") {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file {path}"))?;
        let toml: toml::Table = content
            .parse()
            .with_context(|| format!("Failed to parse toml config from {path}"))?;

        if let Some(v) = toml.get("user") {
            user = Some(v.as_str().context("user must be string")?.to_string())
        };
        if let Some(v) = toml.get("log_level") {
            log_level = Some(v.as_str().context("log_level must be string")?.to_string())
        };
        if let Some(v) = toml.get("disable_timestamps") {
            if !disable_timestamps {
                disable_timestamps = v.as_bool().context("disable_timestamps must be bool")?
            }
        };
        if let Some(v) = toml.get("local_address") {
            if local_address.is_none() {
                local_address = Some(
                    v.as_str()
                        .context("local_address must be string")?
                        .parse()
                        .context("Failed to parse local_address")?,
                )
            }
        };
        if let Some(v) = toml.get("remote_address") {
            if remote_address.is_none() {
                remote_address = Some(
                    v.as_str()
                        .context("remote_address must be string")?
                        .parse()
                        .context("Failed to parse remote_address")?,
                )
            }
        };
        if let Some(v) = toml.get("xor_key") {
            if xor_key.is_none() {
                xor_key = Some(v.as_str().context("xor_key must be string")?.to_string())
            }
        };
    }

    let local_address = local_address.context("local_address is not set")?;
    let remote_address = remote_address.context("remote_address is not set")?;
    let xor_key = xor_key.context("xor_key is not set")?;
    Ok(Config {
        user,
        log_level,
        disable_timestamps,
        local_address,
        remote_address,
        xor_key,
    })
}
