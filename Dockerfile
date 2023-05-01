# TODO: go back to alpine once libmp3lame dependency is fixed
FROM rust:1.69-slim-buster AS build

RUN apt update
RUN apt install -y libmp3lame-dev libvorbis-dev libjemalloc-dev build-essential

RUN mkdir -p /workspace/src
WORKDIR /workspace

ADD src /workspace/src
ADD Cargo.toml Cargo.lock /workspace

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN --mount=type=cache,target=/workspace/target cargo build --release && cp target/release/edicast .



FROM debian:buster-slim AS deploy

RUN apt update
RUN apt install -y libmp3lame0 libvorbis0a libjemalloc2

COPY --from=build /workspace/edicast /usr/local/bin/
ADD edicast.production.toml /etc/edicast.toml

CMD ["/usr/local/bin/edicast", "/etc/edicast.toml"]
