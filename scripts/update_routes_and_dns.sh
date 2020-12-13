#!/usr/bin/env bash

set -x

export PATH=$PATH:/sbin:/usr/sbin:/bin:/usr/bin
IP_EXEC=$(type -p ip)
RESOLVCONF=$(type -p resolvconf)

case $script_type in

up)
  # Route server ip to old default
  SERVER_VIA_DEV_TEXT=$($IP_EXEC route get $server_ip | sed -n -e 's/^.*via \([^ ]\+\).* dev \([^ ]\+\).*/\1 \2/p')
  SERVER_VIA_DEV=($SERVER_VIA_DEV_TEXT);
  SERVER_VIA=${SERVER_VIA_DEV[0]}
  SERVER_DEV=${SERVER_VIA_DEV[1]}
  $IP_EXEC route add $server_ip via $SERVER_VIA dev $SERVER_DEV

  # Add the new default route
  # Add two seperate routes to override the old default, so we don't need to delete the old default, same as what openvpn does.
  $IP_EXEC route add 0.0.0.0/1 via $peer_ip dev $dev 
  $IP_EXEC route add 128.0.0.0/1 via $peer_ip dev $dev

  # Apply DNS
  R="nameserver 8.8.8.8"
  echo -n "$R" | $RESOLVCONF -x -a "$dev.inet"
  ;;
down)
  $RESOLVCONF -d "$dev.inet"

  $IP_EXEC route del $server_ip
  ;;
esac
