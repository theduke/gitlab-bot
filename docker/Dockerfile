# -*- mode: dockerfile -*-
#
# An example Dockerfile showing how to build a Rust executable using this
# image, and deploy it with a tiny Alpine Linux container.

# Our first FROM statement declares the build environment.
FROM ekidd/rust-musl-builder:nightly AS builder

ADD ./Cargo.toml ./
RUN mkdir src && touch src/main.rs && (cargo check || echo "") && rm -r src Cargo.toml

# Add our source code.
ADD . ./

# Fix permissions on source code.
# RUN sudo chown -R rust:rust /home/rust

# Build our application.
RUN cargo build --release

# Now, we need to build our _real_ Docker container, copying in `using-diesel`.
FROM alpine:latest
RUN apk --no-cache add ca-certificates
COPY --from=builder \
    /home/rust/src/target/x86_64-unknown-linux-musl/release/gitlab-bot \
    /opt/gitlab-bot
CMD /opt/gitlab-bot

