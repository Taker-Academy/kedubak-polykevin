FROM rust:1.57.0-alpine as builder

RUN apk add --no-cache musl-dev
RUN apk add pkgconfig
RUN apk add openssl openssl-dev alpine-sdk musl-dev gcc
RUN apk add perl
## Statically link binary to OpenSSL libraries.
ENV OPENSSL_STATIC=yes \
    AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR=/usr/lib/aarch64-linux-gnu \
    AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_INCLUDE_DIR=/usr/include/aarch64-linux-gnu \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ \
    PKG_CONFIG_PATH="/usr/lib/aarch64-linux-gnu/pkgconfig/:${PKG_CONFIG_PATH}"
WORKDIR /opt
RUN rustup override set nightly
RUN rustup target add x86_64-unknown-linux-musl
RUN USER=root cargo new --bin kedubak
WORKDIR /opt/kedubak
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
RUN cargo build --release
RUN rm ./src/*.rs
RUN rm ./target/release/deps/kedubak*

ADD ./src ./src
RUN cargo build --release

FROM scratch
WORKDIR /opt/kedubak
COPY --from=builder /opt/kedubak/target/release/kedubak .

EXPOSE 8080
CMD ["/opt/kedubak/kedubak"]
