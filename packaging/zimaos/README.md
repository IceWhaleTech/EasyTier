# ZimaOS Sysroot Packaging

This folder stores templates used to assemble a `build/sysroot` artifact in the following layout:

- `usr/bin/zimaos-easytier-web`
- `usr/bin/zimaos-easytier-core`
- `usr/lib/systemd/system/zimaos-easytier-web.service`
- `usr/lib/systemd/system/zimaos-easytier-core.service`
- `etc/casaos/zimaos-easytier-web.conf.sample`
- `etc/casaos/zimaos-easytier-core.conf.sample`

## Local Build

```bash
bash ./script/build_zimaos_sysroot.sh
```

By default it builds:

- web package/bin: `easytier-web` / `easytier-web` with feature `embed`
- core package/bin: `easytier` / `easytier-core`
- target: current host target from `rustc -vV` (for example `aarch64-apple-darwin` on Apple Silicon Mac)
- output binaries: `zimaos-easytier-web`, `zimaos-easytier-core`

For Linux output matching your sample layout (ELF x86_64), set:

```bash
TARGET_TRIPLE=x86_64-unknown-linux-gnu
```

Override with environment variables:

```bash
TARGET_TRIPLE=x86_64-unknown-linux-gnu \
WEB_CARGO_FEATURES=embed \
CORE_CARGO_FEATURES= \
bash ./script/build_zimaos_sysroot.sh
```
