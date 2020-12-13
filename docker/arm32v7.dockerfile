FROM debian:stretch

RUN apt-get update && apt-get install -y gcc-arm-linux-gnueabihf curl build-essential && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
RUN $HOME/.cargo/bin/rustup target add armv7-unknown-linux-gnueabihf

COPY entrypoint.sh /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

# buster's glibc is too new
#
# FROM rust:1.48-buster

# RUN apt-get update && apt-get install -y gcc-arm-linux-gnueabihf && rm -rf /var/lib/apt/lists/*
# RUN rustup target add armv7-unknown-linux-gnueabihf
