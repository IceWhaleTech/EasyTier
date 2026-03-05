FROM rust:1.93.1-bookworm

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    jq \
    unzip \
    xz-utils \
    wget \
    sudo \
    build-essential \
    pkg-config \
    protobuf-compiler \
    clang \
    llvm \
    libclang-dev \
    musl-tools \
    libappindicator3-dev \
    python3 \
 && rm -rf /var/lib/apt/lists/*

# Keep toolchain behavior stable in CI.
RUN rustup set auto-self-update disable \
 && rustup default 1.93.1 \
 && rustup target add x86_64-unknown-linux-gnu

WORKDIR /work
