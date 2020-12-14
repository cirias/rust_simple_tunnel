## Install

### Build with Docker

```
make docker_image_arm32v7
make docker_build_arm32v7
```

### Install client

```
sudo cp target/armv7-unknown-linux-gnueabihf/release/tunnel /usr/local/bin/

# Remember to update the `ExecStart` for command args
sudo cp systemd/client.service /etc/systemd/system/simple_tunnel.service

sudo systemctl start simple_tunnel.service
```

## Test

### Server Side

```
make docker_server

# ipv4 forwarding needs to be enabled, and this image does it by default
# sysctl -w net.ipv4.ip_forward=1
# edit `/etc/sysctl.conf` for permanent change

iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
RUST_LOG=simple_tunnel=debug ./target/release/tunnel server &
```

### Client Side

```
make docker_client

ip route del default

RUST_LOG=simple_tunnel=debug ./target/release/tunnel client -s 172.17.0.2:3000 &
ip route add default via 192.168.200.1 dev tun0
```

## TODO

- [x] A script for client side initialization
  - Route server ip to old default interface `ip route add <server_ip> dev eth0 via <gateway_ip>`
  - Add a new default route with `ip route add default via 192.168.200.1 dev tun0`
  -  Update `/etc/resolv.conf` with `echo -n "$R" | $RESOLVCONF -x -a "${dev}.inet"` where
    - `$R` is `nameserver 8.8.8.8`
    - `$dev` is `tun?`
  - Add a way to call this script in client mode, for both up and down
- [ ] Shutdown gracefully
  - Delete the specific route for server ip
  - Revert the DNS
- [ ] Server should retry(restart listening) immediately after a fail
- [ ] Handle IPv6
- [x] ~ Build a arm32v7 docker image for client to run on Raspberry PI ~
  - https://github.com/multiarch/qemu-user-static There is a [bug of QEMU with large filesystem](https://github.com/rust-lang/cargo/issues/7451)
