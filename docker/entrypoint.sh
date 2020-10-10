#!/bin/bash

set -e

source $HOME/.cargo/env

mkdir /dev/net && \
  mknod /dev/net/tun c 10 200

exec "$@"
