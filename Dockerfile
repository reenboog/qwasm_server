FROM messense/rust-musl-cross:x86_64-musl as builder

# RUN apk add --no-cache ca-certificates

WORKDIR /uploader

COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

RUN mkdir src
RUN echo 'fn main() { println!("Hello, world!"); }' > src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-musl || true
RUN rm -rf src

COPY ./src ./src

ARG USE_TLS
ENV USE_TLS=${USE_TLS}

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch

# TODO: extract aws only certs, if possible
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

COPY --from=builder uploader/target/x86_64-unknown-linux-musl/release/uploader /uploader
ENTRYPOINT ["/uploader"]
EXPOSE 3000