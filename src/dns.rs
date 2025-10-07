use anyhow::Context;

#[derive(Default, Debug, serde::Deserialize)]
#[serde(default)]
pub struct ResolveOptions {
    pub ipv4_only: bool,
    pub ipv6_only: bool,
}

pub async fn resolve_and_filter_ips(
    addresses: &Vec<String>,
    resolve_options: &ResolveOptions,
) -> anyhow::Result<Vec<std::net::SocketAddr>> {
    if addresses.is_empty() {
        anyhow::bail!("addresses must not be empty!");
    }
    let mut ret = Vec::new();
    for addr in addresses.iter() {
        let ips = tokio::net::lookup_host(&addr)
            .await
            .with_context(|| format!("Cannot resolve '{addr}'"))?;
        ret.extend(ips);
    }
    if ret.is_empty() {
        anyhow::bail!("Cannot resolve any of {addresses:?}");
    }
    if resolve_options.ipv4_only {
        ret.retain(|a| a.is_ipv4());
        if ret.is_empty() {
            anyhow::bail!(
                "Config requested ipv4_only, but no IPv4 addresses found for {addresses:?}"
            );
        }
    } else if resolve_options.ipv6_only {
        ret.retain(|a| a.is_ipv6());
        if ret.is_empty() {
            anyhow::bail!(
                "Config requested ipv6_only, but no IPv6 addresses found for {addresses:?}"
            );
        }
    }
    Ok(ret)
}

#[cfg(test)]
mod test_resolve {
    use super::*;

    #[tokio::test]
    async fn resolve_all() {
        let ips = resolve_and_filter_ips(&vec!["localhost:443".to_string()], &Default::default())
            .await
            .unwrap();
        assert!(ips.contains(&"127.0.0.1:443".parse().unwrap()));
        assert!(ips.contains(&"[::1]:443".parse().unwrap()));
    }

    #[tokio::test]
    async fn filter_ipv4() {
        let ips = resolve_and_filter_ips(
            &vec!["localhost:443".to_string()],
            &ResolveOptions {
                ipv4_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(ips.contains(&"127.0.0.1:443".parse().unwrap()));
        assert!(!ips.contains(&"[::1]:443".parse().unwrap()));
    }

    #[tokio::test]
    async fn filter_ipv6() {
        let ips = resolve_and_filter_ips(
            &vec!["localhost:443".to_string()],
            &ResolveOptions {
                ipv6_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(!ips.contains(&"127.0.0.1:443".parse().unwrap()));
        assert!(ips.contains(&"[::1]:443".parse().unwrap()));
    }
}
