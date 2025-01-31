# udp-obfuscat

This is an UDP proxy with a simple xor cipher obfuscation in Rust.

## Help

```bash
Usage: udp-obfuscat [OPTIONS]

Options:
  -c, --config-file <FILE>
          Sets a custom config file
  -l, --local-address <LOCAL_ADDRESS>
          Where to bind listening client or server UDP socket
  -r, --remote-address <REMOTE_ADDRESS>
          Address of an udp-obfuscat server in client mode or UDP upstream in server mode
      --xor-key <XOR_KEY>
          Base64-encoded key for a Xor filter
      --head-len <HEAD_LEN>
          Apply filter to only first head_len bytes of each packet
      --disable-timestamps
          Disable timestamps in log messages
  -h, --help
          Print help (see more with '--help')
  -V, --version
          Print version
```

Options in command line override the same options from a file. Additional toml options:

- user - string, switch to this user when running as root to drop privileges;
- log_level - string, log level for env_logger. Takes same values as
  log::LevelFilter
  [enum](https://docs.rs/log/0.4.20/log/enum.LevelFilter.html);
- journald - bool, use systemd-journal instead of env_logger.

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
