#!/bin/bash

set -e

[ -f "$HOME/.cargo/env" ] && source $HOME/.cargo/env

mkdir /dev/net && \
  mknod /dev/net/tun c 10 200

exec "$@"
