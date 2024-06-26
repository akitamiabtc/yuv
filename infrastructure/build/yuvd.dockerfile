# Start with a rust alpine image
FROM rust:1.74-alpine3.18 as builder
# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV RUSTFLAGS="-C target-feature=-crt-static"
# if needed, add additional dependencies here
RUN apk add --no-cache musl-dev openssl-dev

WORKDIR /opt

COPY Cargo.lock .
COPY Cargo.toml .

# Remove benches and tests from Cargo.toml to not affect build on their change.
RUN sed -i '/"benches"/,/"tests"/d' Cargo.toml

COPY crates crates/
COPY apps apps/

RUN --mount=type=cache,target=/root/.cargo/registry --mount=type=cache,target=/root/.cargo/git --mount=type=cache,target=/opt/target \
	cargo build --release -p yuvd \
	&& mkdir out \
	&& cp target/release/yuvd out/ \
	&& strip out/yuvd

# use a plain alpine image, the alpine version needs to match the builder
FROM alpine:3.18

# if needed, install additional dependencies here
RUN apk add --no-cache libgcc

# Copy our build
COPY --from=builder /opt/out/yuvd /bin/yuvd

CMD ["/bin/yuvd", "run", "--config", "/config.toml"]
