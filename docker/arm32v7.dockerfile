FROM arm32v7/rust:latest as Builder

RUN apt-get update && apt-get install pkg-config libssl-dev -y

WORKDIR /app
COPY .cargo Cargo.* ./
COPY src ./src
RUN cargo build --release


FROM arm32v7/debian:latest as Runner

COPY docker/entrypoint.sh /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

COPY --from=Builder /app/target/release/tunnel /usr/local/bin/

CMD ["tunnel"]
