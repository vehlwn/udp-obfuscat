# udp-obfuscat

This is an UDP proxy with a simple xor cipher obfuscation in Rust.

## Help

```bash
udp-obfuscat client and server

Usage: udp-obfuscat --local-address <LOCAL_ADDRESS> --remote-address <REMOTE_ADDRESS> --xor-key <XOR_KEY>

Options:
  -l, --local-address <LOCAL_ADDRESS>    Where to bind listening client UDP socket
  -r, --remote-address <REMOTE_ADDRESS>  Address of an udp-obfuscat server
  -x, --xor-key <XOR_KEY>                Base64-encoded key for a Xor filter
  -h, --help                             Print help
  -V, --version                          Print version
```

## Examples

You can generate xor-key with openssl:

```bash
openssl rand -base64 16
```

### Client

```bash
$ RUST_LOG=trace cargo run -- -l 127.0.0.1:5050 -r 192.0.2.1:5050 --xor-key aaaa
```

### Server

```bash
$ RUST_LOG=trace cargo run -- -l 192.0.2.1:5050 -r 127.0.0.1:6060 --xor-key aaaa
```

Now on a client side packets sent to 127.0.0.1:5050 will be forwarded to
127.0.0.1:6060 on a server side.

![Diagram](diagram.png)
