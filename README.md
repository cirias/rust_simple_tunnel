## Test

### Server Side

```
make docker_server

# ipv4 forwarding needs to be enabled, and this image does it by default
# sysctl -w net.ipv4.ip_forward=1
# edit `/etc/sysctl.conf` for permanent change

iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
RUST_LOG=simple_vpn=debug ./target/release/tunnel server &
```

### Client Side

```
make docker_client

ip route del default

RUST_LOG=simple_vpn=debug ./target/release/tunnel client -s 172.17.0.2:3000 &
ip route add default via 192.168.200.1 dev tun0
```

## TODO

- [ ] A script for client side initialization
  - Route server ip to old default interface `ip route add <server_ip> dev eth0 via <gateway_ip>`
  - Add a new default route with `ip route add default via 192.168.200.1 dev tun0`
  -  Update `/etc/resolv.conf` with `echo -n "$R" | $RESOLVCONF -x -a "${dev}.inet"` where
    - `$R` is `nameserver 8.8.8.8`
    - `$dev` is `tun?`
  - Add a way to call this script in client mode, for both up and down
- [ ] Build a arm32v7 docker image for client to run on Raspberry PI
  - https://github.com/multiarch/qemu-user-static
- [ ] Server should retry(restart listening) immediately after a fail
