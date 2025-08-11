use std::sync::Arc;

use super::*;
use crate::config::{ListenerOptions, RemoteOptions};
use crate::filters::{Head, IFilter, Xor};

#[tokio::test]
async fn proxy_transforms() {
    let proxy_addr = "127.0.0.1:6060";
    let upstream_addr = "127.0.0.1:7070";
    let filter: Box<IFilter> = Box::new(Head::new(Box::new(Xor::with_key(vec![3])), 3));
    let proxy = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: vec![proxy_addr.to_string()],
                resolve_options: Default::default(),
            },
            &RemoteOptions {
                address: upstream_addr.to_string(),
                resolve_options: Default::default(),
            },
            filter,
        )
        .await
        .unwrap(),
    );
    let upstream_task = async move {
        let listener = tokio::net::UdpSocket::bind(upstream_addr).await.unwrap();
        let mut read_buf = crate::common::datagram_buffer();
        let (recv_len, _) = listener.recv_from(read_buf.as_mut()).await.unwrap();
        let data = &read_buf[..recv_len];
        // Server xored only the first 3 bytes: (3 ^ 7) == 4.
        assert_eq!(data, [4, 4, 4, 7, 7, 7, 7, 7]);
    };
    let proxy_task = async move {
        proxy.run().await.unwrap();
    };

    let client_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_sock.connect(proxy_addr).await.unwrap();
    client_sock.send(&[7_u8; 8]).await.unwrap();
    tokio::select! {
        _ = upstream_task => {}
        _ = proxy_task => {}
    }
}

#[tokio::test]
async fn proxy_proxies() {
    let proxy_client_addr = "127.0.0.1:6061";
    let proxy_server_addr = "127.0.0.1:6071";
    let upstream_addr = "127.0.0.1:7071";

    let key_data = vec![3];
    let filter_client: Box<IFilter> =
        Box::new(Head::new(Box::new(Xor::with_key(key_data.clone())), 3));
    let filter_server: Box<IFilter> =
        Box::new(Head::new(Box::new(Xor::with_key(key_data.clone())), 3));

    let proxy_client = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: vec![proxy_client_addr.to_string()],
                resolve_options: Default::default(),
            },
            &RemoteOptions {
                address: proxy_server_addr.to_string(),
                resolve_options: Default::default(),
            },
            filter_client,
        )
        .await
        .unwrap(),
    );
    let proxy_server = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: vec![proxy_server_addr.to_string()],
                resolve_options: Default::default(),
            },
            &RemoteOptions {
                address: upstream_addr.to_string(),
                resolve_options: Default::default(),
            },
            filter_server,
        )
        .await
        .unwrap(),
    );

    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let upstream_task = async move {
        let listener = tokio::net::UdpSocket::bind(upstream_addr).await.unwrap();
        let mut read_buf = crate::common::datagram_buffer();
        let (recv_len, peer) = listener.recv_from(read_buf.as_mut()).await.unwrap();
        let data = &read_buf[..recv_len];
        assert_eq!(data, b"hello from client");
        listener
            .send_to(b"hello from upstream", peer)
            .await
            .unwrap();
        // Must wait until client finishes
        done_rx.await.unwrap();
    };
    let proxy_client_task = async move {
        proxy_client.run().await.unwrap();
    };
    let proxy_server_task = async move {
        proxy_server.run().await.unwrap();
    };

    let client_task = async move {
        let client_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_sock.connect(proxy_client_addr).await.unwrap();
        client_sock.send(b"hello from client").await.unwrap();
        let mut read_buf = crate::common::datagram_buffer();
        let n = client_sock.recv(read_buf.as_mut()).await.unwrap();
        assert_eq!(&read_buf[..n], b"hello from upstream");
        done_tx.send(()).unwrap();
    };

    tokio::select! {
        _ = upstream_task => {}
        _ = proxy_client_task => {}
        _ = proxy_server_task => {}
        _ = client_task => {}
    }
}

#[tokio::test]
async fn local_address_ipv4() {
    let proxy_addr: Vec<String> = ["127.0.0.1:6062", "[::1]:6062"]
        .iter()
        .map(|x| x.to_string())
        .collect();
    let upstream_addr = "127.0.0.1:7070";
    let filter: Box<IFilter> = Box::new(Xor::with_key(vec![]));
    let proxy = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: proxy_addr,
                resolve_options: dns::ResolveOptions {
                    ipv4_only: true,
                    ..Default::default()
                },
            },
            &RemoteOptions {
                address: upstream_addr.to_string(),
                resolve_options: Default::default(),
            },
            filter,
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        proxy.get_local_address(),
        ["127.0.0.1:6062".parse().unwrap()]
    );
}

#[tokio::test]
async fn local_address_ipv6() {
    let proxy_addr: Vec<String> = ["127.0.0.1:6062", "[::1]:6062"]
        .iter()
        .map(|x| x.to_string())
        .collect();
    let upstream_addr = "127.0.0.1:7070";
    let filter: Box<IFilter> = Box::new(Xor::with_key(vec![]));
    let proxy = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: proxy_addr,
                resolve_options: dns::ResolveOptions {
                    ipv6_only: true,
                    ..Default::default()
                },
            },
            &RemoteOptions {
                address: upstream_addr.to_string(),
                resolve_options: Default::default(),
            },
            filter,
        )
        .await
        .unwrap(),
    );
    assert_eq!(proxy.get_local_address(), ["[::1]:6062".parse().unwrap()]);
}

#[tokio::test]
async fn remote_address_ipv4() {
    let proxy_addr = vec!["localhost:6063".to_string()];
    let upstream_addr = "localhost:7070";
    let filter: Box<IFilter> = Box::new(Xor::with_key(vec![]));
    let proxy = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: proxy_addr,
                resolve_options: Default::default(),
            },
            &RemoteOptions {
                address: upstream_addr.to_string(),
                resolve_options: dns::ResolveOptions {
                    ipv4_only: true,
                    ..Default::default()
                },
            },
            filter,
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        proxy.get_remote_address(),
        ["127.0.0.1:7070".parse().unwrap()]
    );
}

#[tokio::test]
async fn remote_address_ipv6() {
    let proxy_addr = vec!["localhost:6064".to_string()];
    let upstream_addr = "localhost:7070";
    let filter: Box<IFilter> = Box::new(Xor::with_key(vec![]));
    let proxy = Arc::new(
        UdpProxy::new(
            &ListenerOptions {
                address: proxy_addr,
                resolve_options: Default::default(),
            },
            &RemoteOptions {
                address: upstream_addr.to_string(),
                resolve_options: dns::ResolveOptions {
                    ipv6_only: true,
                    ..Default::default()
                },
            },
            filter,
        )
        .await
        .unwrap(),
    );
    assert_eq!(proxy.get_remote_address(), ["[::1]:7070".parse().unwrap()]);
}
