<!--
# SPDX-FileCopyrightText: 2023 aerynOS Developers
# SPDX-License-Identifier: MPL-2.0
-->

# aerynOS OS Tools

The core package management and building tools for [aerynOS](https://aerynos.com) written in Rust along with the required shared libraries used to make it all work and centered around aerynOS' native `.stone` package format.

This repository focuses on two tools:

- **`moss`** — package manager and system state manager. Resolves package transactions, manages content-addressable storage and composes system updates.
- **`boulder`** — package builder and recipe manager. Builds upstream software in isolated environments and produces native `.stone` packages.

### moss

`moss` is the native aerynOS package and system state manager. It works via an underlying content-addressable store that keeps a single copy of unique files contained within package archives. During atomic transactions, it creates new a new /usr filesystem tree from the associated package metadata and layout information for all packages required in the next system state.

This approach allows `moss` to efficiently keep all package contents for many transactions and allows users to rollback to earlier transactions without disproportionately taking up storage space for full system images.

### boulder

`boulder` is the native aerynOS builder tooling to turn upstream software into native aerynOS `.stone `packages via strongly defined YAML recipes. It uses rootless Linux containers to isolate package builds from the host system. `boulder` automatically populates recipes with as much metadata as it can in order to simplify the overall packaging process, freeing packagers up to focus on the remaining packaging work that cannot be trivially automated away.

### `.stone`

`.stone` is the native aerynOS package format. It separates package metadata, filesystem layout, content indexing and file data, enabling `moss` and `boulder` to work with package contents independently of their final filesystem representation.

## Installation

### Requirements

- Linux development environment
- Rust toolchain (version specified in `rust-toolchain.toml`)
- Cargo
- [just](https://github.com/casey/just)
- Suitable subordinate UID/GID ranges configured in `/etc/subuid` and `/etc/subgid` for boulder's rootless build environments

#### subuid and subgid support
If you do not already have subuid and subgid support set up for your ${USER} in /etc/subuid and /etc/subuid then you can use the following commands

```
sudo touch /etc/sub{uid,gid}
sudo usermod --add-subuids 1000000-1065535 --add-subgids 1000000-1065535 root
sudo usermod --add-subuids 1065536-1131071 --add-subgids 1065536-1131071 ${USER}
```

### Onboarding

The easiest way to build and install both tools locally is through the following onboarding steps.

```bash
# clone the aerynOS os-tools repo somewhere reasonable
mkdir -pv ~/Projects/aerynos/
cd ~/Projects/aerynos/
git clone https://github.com/aerynOS/os-tools.git
cd os-tools/

# Install a few prerequisites (this how you'd do it on aerynOS)
sudo moss pkg add rust binutils glibc-devel linux-headers clang bsdtar-static cpio

# remember to add ~/.cargo/bin to your $PATH if this is how you installed rustfmt
cargo install rustfmt
cargo install typos-cli --locked

# from inside the os-tools directory, you can build boulder and moss
# and install them to ${HOME}/.local/bin/ by default
just get-started
```

This builds moss and boulder and installs their executables and supporting data into your local user environment.

**NB:** If you want to build .stones with boulder on your _non-aerynOS_ host system, you will need to specify the location of the boulder data files (which live in ${HOME}/.local/share/boulder if you used `just get-started` like above):

```bash
alias boulder="${HOME}/.local/bin/boulder --data-dir=${HOME}/.local/share/boulder/ --config-dir=${HOME}/.config/boulder/ --moss-root=${HOME}/.cache/boulder/"
```

### Working with just

Common development tasks are available through `just`:

| Command | Description |
|---|---|
| `just get-started` | Build and install `moss` and `boulder` |
| `just moss` | Build `moss` |
| `just boulder` | Build `boulder` |
| `just lint` | Run linting and formatting checks |
| `just test` | Run linting and the test suite |
| `just fix` | Apply automatic code fixes |

## Experimenting and using `moss`

**NB:** Remember to use the `-D aosroot/` argument to specify a root directory, otherwise `moss` will happily eat your current operating system!

```bash
# Once moss in installed create the aosroot/ directory
mkdir -pv aosroot/

# Add the unstable repo
moss -D aosroot/ repo add unstable https://cdn.aerynos.dev --root-index channel=main,version=stream/unstable -p 0
# and/or add the volatile repo
moss -D aosroot/ repo add volatile https://cdn.aerynos.dev --root-index channel=main,version=stream/volatile -p 10

# List available packages
moss -D aosroot/ pkg list --available

# Install packages with moss
moss -D aosroot/ pkg add systemd bash libx11-32bit

# A full list of commands can be found by typing `moss --help`
```

If you want to create systemd-nspawn roots or bootable VMs, please check out the [img-tests](https://github.com/aerynOS/img-tests) repository.

## Contributing

We happily welcome contributions to our codebase, so long as they are made inline with our [Contribution guidelines](https://github.com/aerynOS/os-tools/?tab=contributing-ov-file). Further documentation can be found at our [documentation site](https://aerynos.com/aerynos/) and should help your overall understanding of our codebase and project goals.

Before submitting any changes to our codebase, please ensure all tests are running locally without issue:

```bash
# Prior to committing a change:
just test # includes the just lint target

# Prior to pushing anything, apply clippy fixes:
just fix
```

Once this has been done, you can create a Pull Request with your changes.

### We do not accept contributions authored using genAI/LLM chatbots/agents

Our stance is that we are here to help train people's minds and skills. As an analogy, it would defeat the purpose if you brought an industrial robot with you to the gym so it could lift weights for you.

For more details, consult the [LLM section](https://github.com/aerynOS/os-tools?tab=contributing-ov-file#llm-contributions) of our contribution guidelines.

## License

Licensed under the terms of the [MPL-2.0](https://spdx.org/licenses/MPL-2.0.html).
