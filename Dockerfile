FROM messense/rust-musl-cross:x86_64-musl as builder

WORKDIR /uploader

COPY ./Cargo.toml ./Cargo.toml
RUN ls ./Cargo.lock && cp ./Cargo.lock ./ || true
COPY ./src ./src

ARG USE_TLS
ENV USE_TLS=${USE_TLS}

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder uploader/target/x86_64-unknown-linux-musl/release/uploader /uploader
COPY ./certs /certs
ENTRYPOINT ["/uploader"]
EXPOSE 3000