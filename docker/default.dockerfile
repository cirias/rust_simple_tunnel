FROM debian:latest

RUN apt-get update && \
  apt-get install -y \
    iptables \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    tcpdump \
    iperf3 \
    procps

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

COPY entrypoint.sh /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

CMD ["/bin/bash"]
