## Test

### Server Side

```
make docker_server

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
