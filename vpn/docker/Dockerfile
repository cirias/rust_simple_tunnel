FROM debian:latest

RUN  apt-get update && apt-get install iptables -y

COPY entrypoint.sh /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

CMD ["/bin/bash"]
