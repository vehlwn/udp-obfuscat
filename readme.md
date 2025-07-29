# udp-obfuscat

This is an UDP proxy with a simple xor cipher obfuscation in Rust.

## Help

```bash
Usage: udp-obfuscat --config-file <FILE>

Options:
  -c, --config-file <FILE>  Read options from a config file
  -h, --help                Print help (see more with '--help')
  -V, --version             Print version
```

## Config

See [src/config.rs] for toml schema and definitions.

## Examples

You can generate xor-key with openssl:

```bash
openssl rand -base64 16
```

### Client

```bash
$ RUST_LOG=trace cargo run -- -c <(cat << EOF
[listener]
address = ["localhost:5050"]
[remote]
address = "192.0.2.1:5050"
[filters]
xor_key = "aaaa"
EOF
)
```

### Server

```bash
$ RUST_LOG=trace cargo run -- -c <(cat << EOF
[listener]
address = ["192.0.2.1:5050"]
[remote]
address = "127.0.0.1:6060"
[filters]
xor_key = "aaaa"
EOF
)
```

Now on a client side packets sent to 127.0.0.1:5050 or [::1]:5050 will be
forwarded to 127.0.0.1:6060 on a server side.

![Diagram](diagram.png)
