FROM rust:1.48-buster as builder

WORKDIR /app
COPY Cargo.* ./
COPY .cargo ./cargo
COPY src ./src

RUN cargo build --release


FROM debian:buster as runner

RUN apt-get update \
  && apt-get install -y \
    openresolv \
    curl \
    supervisor \
    shadowsocks-libev \
  && rm -rf /var/lib/apt/lists/*

COPY docker/shadowsocks/supervisord.conf /etc/supervisor/conf.d/supervisord.conf
COPY docker/shadowsocks/entrypoint.sh /usr/local/bin/

COPY scripts /etc/tunnel/scripts
COPY --from=builder /app/target/release/tunnel /usr/local/bin/

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

CMD ["/usr/bin/supervisord"]
