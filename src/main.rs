mod common;
mod config;
mod filters;
mod init_logging;
mod proxy;

use anyhow::Context;

use std::sync::Arc;

fn drop_root(user: nix::unistd::User) -> anyhow::Result<()> {
    log::debug!(
        "Dropping root privileges to UID {}, GID {}",
        user.uid,
        user.gid
    );
    nix::unistd::setgroups(&[]).context("setgroups failed")?;
    nix::unistd::setgid(user.gid).context("setgid failed")?;
    nix::unistd::setuid(user.uid).context("setuid failed")?;
    Ok(())
}

fn make_filter(config: &crate::config::Config) -> anyhow::Result<Box<crate::filters::IFilter>> {
    use base64::prelude::*;
    let xor_key = BASE64_STANDARD
        .decode(config.xor_key.as_bytes())
        .context("Failed to convert xor_key from base64")?;

    let mut ret: Box<crate::filters::IFilter> = Box::new(crate::filters::Xor::with_key(xor_key));
    if let Some(n) = config.head_len {
        ret = Box::new(crate::filters::Head::new(ret, n));
    }
    return Ok(ret);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use config::parse_config;

    let config = parse_config().context("Failed to parse config")?;
    init_logging::init_logging(&config)?;
    log::debug!("{config:?}");

    let filter = make_filter(&config)?;
    let udp_proxy = Arc::new(
        crate::proxy::UdpProxy::new(config.local_address, config.remote_address, filter).await?,
    );

    if let Some(user) = config.user {
        let context = || format!("Failed to get user info for user '{user}'");
        let user = nix::unistd::User::from_name(&user)
            .with_context(context)?
            .with_context(context)?;
        if nix::unistd::Uid::effective().is_root() && !user.uid.is_root() {
            drop_root(user).context("drop_root failed")?;
        }
    }

    log::info!(
        "Listener bound to {}/udp and connected to {}/udp",
        udp_proxy.get_local_address(),
        udp_proxy.get_remote_address()
    );

    udp_proxy.run().await?;

    Ok(())
}
