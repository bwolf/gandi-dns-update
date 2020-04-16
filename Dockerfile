FROM ekidd/rust-musl-builder:1.42.0-openssl11 AS builder
COPY . /home/rust/src
RUN cargo build --release \
    && cp -a target/x86_64-unknown-linux-musl/release/gandi-dns-update /home/rust/app \
    && ls -l /home/rust/app \
    && strip /home/rust/app \
    && echo After strip \
    && ls -l /home/rust/app

# Create final image
FROM scratch
COPY --from=builder /home/rust/app /app
USER 1000
CMD ["./app"]
