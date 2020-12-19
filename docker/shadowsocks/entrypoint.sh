#!/bin/bash

set -e

mkdir /dev/net && \
  mknod /dev/net/tun c 10 200


# let the return packets from shadowsocks bypass default tun route
IP_EXEC=$(type -p ip)
TEST_IP=1.1.1.1
ROUTE_TABLE=10
VIA_DEV_SRC_TEST=$($IP_EXEC route get $TEST_IP | sed -n -e 's/^.*via \([^ ]\+\).* dev \([^ ]\+\).* src \([^ ]\+\).*/\1 \2 \3/p')
VIA_DEV_SRC=($VIA_DEV_SRC_TEST)
VIA=${VIA_DEV_SRC[0]}
DEV=${VIA_DEV_SRC[1]}
SRC=${VIA_DEV_SRC[2]}
$IP_EXEC route add default via $VIA dev $DEV table $ROUTE_TABLE
ip rule add from $SRC sport $SS_PORT table $ROUTE_TABLE

exec "$@"
