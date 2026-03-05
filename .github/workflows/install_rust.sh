#!/usr/bin/env bash

# env needed:
# - TARGET
# - GUI_TARGET
# - OS

# dependencies are only needed on ubuntu as that's the only place where
# we make cross-compilation
if [[ $OS =~ ^ubuntu.*$ ]]; then
    sudo apt-get update && sudo apt-get install -qq musl-tools libappindicator3-dev llvm clang
    # https://github.com/cross-tools/musl-cross/releases
    # if "musl" is a substring of TARGET, we assume that we are using musl
    MUSL_TARGET=$TARGET
    # if target is mips or mipsel, we should use soft-float version of musl
    if [[ $TARGET =~ ^mips.*$ || $TARGET =~ ^mipsel.*$ ]]; then
        MUSL_TARGET=${TARGET}sf
    elif [[ $TARGET =~ ^riscv64gc-.*$ ]]; then
        MUSL_TARGET=${TARGET/#riscv64gc-/riscv64-}
    fi
    if [[ $MUSL_TARGET =~ musl ]]; then
        mkdir -p ./musl_gcc
        wget --inet4-only -c https://github.com/cross-tools/musl-cross/releases/download/20250520/${MUSL_TARGET}.tar.xz -P ./musl_gcc/
        tar xf ./musl_gcc/${MUSL_TARGET}.tar.xz -C ./musl_gcc/
        sudo ln -sf $(pwd)/musl_gcc/${MUSL_TARGET}/bin/*gcc /usr/bin/
        sudo ln -sf $(pwd)/musl_gcc/${MUSL_TARGET}/include/ /usr/include/musl-cross
        sudo ln -sf $(pwd)/musl_gcc/${MUSL_TARGET}/${MUSL_TARGET}/sysroot/ ./musl_gcc/sysroot
        sudo chmod -R a+rwx ./musl_gcc
    fi
fi

# see https://github.com/rust-lang/rustup/issues/3709
rustup set auto-self-update disable
RUST_VERSION=${RUST_VERSION:-1.93}
RUST_VERSION_REGEX="^${RUST_VERSION//./\\.}(\\.|$)"
ACTIVE_RUSTC_VERSION="$(rustc --version | awk '{print $2}')"
if [[ "$ACTIVE_RUSTC_VERSION" =~ $RUST_VERSION_REGEX ]]; then
    echo "rustc $ACTIVE_RUSTC_VERSION already matches required $RUST_VERSION, skip rustup install/default"
else
    rustup install "$RUST_VERSION"
    rustup default "$RUST_VERSION"
fi

# mips/mipsel cannot add target from rustup, need compile by ourselves
if [[ $OS =~ ^ubuntu.*$ && $TARGET =~ ^mips.*$ ]]; then
    cd "$PWD/musl_gcc/${MUSL_TARGET}/lib/gcc/${MUSL_TARGET}/15.1.0" || exit 255
    # for panic-abort
    cp libgcc_eh.a libunwind.a

    # for mimalloc
    ar x libgcc.a _ctzsi2.o _clz.o _bswapsi2.o
    ar rcs libctz.a _ctzsi2.o _clz.o _bswapsi2.o

    rustup toolchain install nightly-2026-02-02-x86_64-unknown-linux-gnu
    rustup component add rust-src --toolchain nightly-2026-02-02-x86_64-unknown-linux-gnu

    # https://github.com/rust-lang/rust/issues/128808
    # remove it after Cargo or rustc fix this.
    RUST_LIB_SRC=$HOME/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/
    if [[ -f $RUST_LIB_SRC/library/Cargo.lock && ! -f $RUST_LIB_SRC/Cargo.lock ]]; then
        cp -f $RUST_LIB_SRC/library/Cargo.lock $RUST_LIB_SRC/Cargo.lock
    fi
else
    if rustup target list --installed | grep -qx "$TARGET"; then
        echo "target $TARGET already installed, skip rustup target add"
    else
        rustup target add $TARGET
    fi
    if [[ $GUI_TARGET != '' ]]; then
        if rustup target list --installed | grep -qx "$GUI_TARGET"; then
            echo "target $GUI_TARGET already installed, skip rustup target add"
        else
            rustup target add $GUI_TARGET
        fi
    fi
fi
