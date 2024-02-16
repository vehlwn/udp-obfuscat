mod common;
mod filters;
mod proxy;

use anyhow::Context;

/// udp-obfuscat client and server
#[derive(Debug, clap::Parser)]
#[clap(version, about, long_about = None)]
struct Args {
    /// Disable timestamps in log messages. By default they are enabled
    #[clap(short, long, env)]
    disable_timestamps: bool,

    /// Where to bind listening client UDP socket
    #[clap(short, long, env)]
    local_address: std::net::SocketAddr,

    /// Address of an udp-obfuscat server
    #[clap(short, long, env)]
    remote_address: std::net::SocketAddr,

    /// Base64-encoded key for a Xor filter
    #[clap(short, long, env)]
    xor_key: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    let args = Args::parse();

    let mut log_builder = env_logger::builder();
    if args.disable_timestamps {
        log_builder.format_timestamp(None);
    }
    log_builder.init();

    use base64::prelude::*;
    let xor_key = BASE64_STANDARD
        .decode(args.xor_key.as_bytes())
        .context("Failed to convert xor_key from base64")?;
    let filter = crate::filters::Xor::with_key(xor_key);
    let udp_proxy = std::sync::Arc::new(
        crate::proxy::UdpProxy::new(args.local_address, args.remote_address, Box::new(filter))
            .await?,
    );

    log::info!(
        "Listener bound to {}/udp and connected to {}/udp",
        udp_proxy.get_local_address(),
        udp_proxy.get_remote_address()
    );

    udp_proxy.run().await?;

    Ok(())
}
