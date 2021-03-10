## Build

### Docker

```
make docker_image
make docker_build
```

### Docker for ARM (Respberry Pi)

```
make docker_image_arm32v7
make docker_build_arm32v7
```

## Setup

Say you have the server with public IP `12.34.56.78`.
And want to name the tun device `tun0` on both client and server.
And let the server use `192.168.200.1` as the virtual IP, client with `192.168.200.2`.

First let's setup the tun device.

```
# On both client and server
ip tuntap add mod tun name tun0

# On server
ip address add dev tun0 192.168.200.1 peer 192.168.200.2
# On client
ip address add dev tun0 192.168.200.2 peer 192.168.200.1

# You may want to set more rules. Here are some examples:
#
### On both client and server
# # enable ip forwarding
# # edit `/etc/sysctl.conf` for permanent change
# sysctl -w net.ipv4.ip_forward=1
#
### On server 
# # turn NAT on
# iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
#
### On client
# # turn NAT on
# iptables -t nat -A POSTROUTING -o tun0 -j MASQUERADE
#
# # route all packets except tunnel's through tun0
# ip route add 12.34.56.78 via <current gateway> dev eth0
# ip route add default via 192.168.200.1 dev tun0
#
# # add DNS
# echo -n "nameserver 1.1.1.1" | resolvconf -x -a "tun0.inet"
```

Then create the TLS certs. See [doc/cert.md](https://github.com/cirias/rust_simple_tunnel/blob/master/doc/cert.md) for the details.
Remember to copy the certs and key to server or client accordingly.

Now let's start `tunnel` on the server.

```
tunnel --tun-name tun0 server --listen 0.0.0.0:443 --username steven --password sekr0t --cert-path cert.pem --key-path key.pem
```

Time to start the `tunnel` on the client. The `--hostname` is just a fake value used in the http request header, choose whatever you want.

```
tunnel --tun-name tun0 server --server 12.34.56.78:443 --hostname www.example.com --username steven --password sekr0t --ca-cert-path ca_cert.pem
```

You can test with a simple ping from both server or client.

```
ping 192.168.200.2 # on server
ping 192.168.200.1 # on client
```

And if everything works fine, you may consider to create a systemd unit for the tunnel process. Here is a template you can start with.

```
# Remember to update the `ExecStart` for command args
cp systemd/tunnel-quick@.service /etc/systemd/system/tunnel-quick@.service
```

## Development Tips

### Local Test Environment

The server

```
make docker_image
make docker_run

ip tuntap add mod tun name tun0
ip address add dev tun0 192.168.200.1 peer 192.168.200.2

RUST_LOG=simple_tunnel=debug ./target/release/tunnel server &
```

The client

```
make docker_image
make docker_run

ip tuntap add mod tun name tun0
ip address add dev tun0 192.168.200.2 peer 192.168.200.1

RUST_LOG=simple_tunnel=debug ./target/release/tunnel client --server 172.17.0.2:3000 &
```

### Useful Commands

```
# useful for stress test
# `-s` set the packet size
# `-i` set the interval of each packet
ping -s 1300 -i 0.01 192.168.200.1
```

#### Network Simulation

- https://wiki.linuxfoundation.org/networking/netem#examples

__NOTE__

```
tc qdisc add dev eth0 root netem delay 100ms
tc qdisc change dev eth0 root netem loss 0.3% 25% delay 100ms 20ms distribution normal

# List rules
tc -p qdisc ls dev eth0

# Delete rules
tc qdisc del dev eth0 root
```
