# No more double copies

## Problem we're solving

Currently, Boulder would first copy Git repos or tarballs into `%(sourcedir)`, and then extract (if `unpack: true`) or copy the repo
into the build directory. This is of course inefficient, although it makes the container setup easier, because there is only one
directory (`%(sourcedir)`) to mount.

## Solution

This PR immediately sets up the bind-mounted paths to point to Git repositories or tarballs, or immediately extract tarballs outside
of the container with `bsdtar`, making builds more efficient.

When it comes to Git repositories, worktrees are used. `.git` for worktree is a text file that contains the path of the parent `.git` directory.
Since such directory must also exists, it gets bind-mounted in R/O mode under the `.git-parent`. name, e.g.:

```text
workdir
  |-- .git
  |-- .git-parent
  |-- source1.rs
  |-- source2.rs
  |-- README.md
```

Of course this mandates the existence of `bsdtar` and `git` on the host system,
but it shouldn't be a problem because `boulder recipe new` already expects that.

## Breaking changes

There is a breaking change introduced with this approach: `%(sourcedir)` is no longer necessary. I don't see it as a problem honestly,
because everything can just be extracted/bind-mounted into `%(workdir)`. `%(sourcedir)` was basically a mere intermediate path before
actual copy/extraction occurred. Anyway, this is a breaking change in that many recipes rely on `%(sourcedir)`, so they must be updated.
It's not a big change, but let's discuss that.

## What about the "Prepare" phase?

The prepare phase was a script run inside the container that copied source into `%(workdir)`.
This is no longer necessary, and in fact
this MR sets the preparation script to the empty string. Should we use the "Prepare" phase for
operations performed by Boulder, instead of ops performed by a script?

## Caveats and future improvements

When a recipe has multiple build targets (e.g. x86_64 and emul32), they share the same container. This approach has a few downsides:

- **Introduced with this PR**: We share the sources upfront for all targets. This means possibly extracting an archive twice.
If the first target fails, we've extracted the source for the second target pointlessly. Recipes with multiple targets are rare,
but still it's an inefficiency.\
The same applies to Git repositories: we create worktrees upfront.

- **Pre-existing behavior**: Targets share the same container. Since we run arbitrary code inside containers, a build script
could contaminate all targets.

I think we should eventually compartmentalize build targets.

---

# Git submodules

This PR removes automatic download of Git submodules. Before it, for each build, submodules are downloaded even when not necessary. This delays deployment/development
and is a security flaw, because it forces the container to have networking.

Submodules are hard and distros notoriously handle them with best effort. Our approach to them is to let packagers add them when required, using the `clonedir` property in recipes. Boulder also helps packagers with an informative message, when an upstream with submodules fails to build. For example, below is the output for `edk2` that I forced to fail to build:

```text
│S│ + exit 1
Hint: Build failed, but this recipe lists Git submodules. Please check if submodules are required.
Declared submodules:
-e230f474b87134e8c6c85b630084c612057f253e BaseTools/Source/C/BrotliCompress/brotli
-0bebf8b8c7f07abe3571ded48a11aa907a1ffb20 CryptoPkg/Library/MbedTlsLib/mbedtls
-8cf17aaeb4599f8af87fefd810b5b5fee90fe69e CryptoPkg/Library/OpensslLib/openssl
-e230f474b87134e8c6c85b630084c612057f253e MdeModulePkg/Library/BrotliCustomDecompressLib/brotli
-4ef89209a239c1aea328cf13c05a2807e5c146d1 MdeModulePkg/Universal/RegularExpressionDxe/oniguruma
-cfff805481bdea27f900c32698171286542b8d3c MdePkg/Library/BaseFdtLib/libfdt
-370b5944c046bab043dd8b133727b2135af7747a MdePkg/Library/MipiSysTLib/mipisyst
-e9ebfa7e77a6bee77df44e096b100e7131044059 RedfishPkg/Library/JsonLib/jansson
-f55cf6d48ec69b4ac60a63903e9c6a2cb0fd155d SecurityPkg/DeviceSecurity/SpdmLib/libspdm
-bc29a21d44b01396223c152a4834e52318591770 TcgTpmPkg/Library/TpmLib/TPM
-1cc9cde3448cdd2e000886a26acf1caac2db7cf1 UnitTestFrameworkPkg/Library/CmockaLib/cmocka
-86add13493e5c881d7e4ba77fb91c1f57752b3a4 UnitTestFrameworkPkg/Library/GoogleTestLib/googletest
-83d4e1ebef3588fae48b69a7352cc21801cb70bc UnitTestFrameworkPkg/Library/SubhookLib/subhook
Error: build: container: exited with failure: run: build recipe: failed with status code 1
```

That one is the output of `git submodule status`. If a recipe fails, *maybe* (but not
necessarily) is because of a missing submodule, and Boulder (recursively) reveals which
one are listed in `.gitmodules` and with with hash.

Suppose we want to add the only submodule that `qt5-declarative` expects. This is the
original list of upstreams:

```yaml
    - git|https://invent.kde.org/qt/qt/qtdeclarative.git  : 1189557a50f11e7bc5716467a149cd09987a9f88
```

The submodule is added with:

```yaml
    - git|https://invent.kde.org/qt/qt/qtdeclarative.git:
        ref: 1189557a50f11e7bc5716467a149cd09987a9f88
        clonedir: qtdeclarative
    - git|https://invent.kde.org/qt/qt/qtdeclarative-testsuites.git:
        ref: 6b0c42c63c2492bd0a7a96d3179d122b5f71793f
        clonedir: qtdeclarative/tests/auto/qml/ecmascripttests/test262
```

---

# Test plan

The following commands were executed in this order, with an empty `~/.cache/boulder` directory.

```shell
alias b=./target/debug/boulder
# Simple tarball-based recipe.
b build ../recipes/n/nano/stone.yaml
# This was tested with both upstream lists as above.
b build ../recipes/q/qt5-declarative/stone.yaml
# This required some modifications. See modified recipe below.
b build ../recipes/e/edk2/stone.yaml
```

edk2 recipe (I don't mean to say that's the correct way to package it! I'm only proving it's buildable):

<details>

```yaml
# SPDX-FileCopyrightText: 2025 AerynOS Developers
# SPDX-License-Identifier: MPL-2.0

name        : edk2
version     : "202608"
release     : 8
upstreams   :
    - git|https://github.com/tianocore/edk2 : 2970e5699ba6267f3384ffab20f96647578aebc8
    - git|https://github.com/google/brotli:
        ref: e230f474b87134e8c6c85b630084c612057f253e
        clonedir: edk2/BaseTools/Source/C/BrotliCompress/brotli
    - git|https://github.com/ARMmbed/mbedtls:
        ref: 0bebf8b8c7f07abe3571ded48a11aa907a1ffb20
        clonedir: edk2/CryptoPkg/Library/MbedTlsLib/mbedtls
    - git|https://github.com/openssl/openssl:
        ref: 8cf17aaeb4599f8af87fefd810b5b5fee90fe69e
        clonedir: edk2/CryptoPkg/Library/OpensslLib/openssl
    - git|https://github.com/kkos/oniguruma:
        ref: 4ef89209a239c1aea328cf13c05a2807e5c146d1
        clonedir: edk2/MdeModulePkg/Universal/RegularExpressionDxe/oniguruma
    - git|https://github.com/devicetree-org/pylibfdt.git:
        ref: cfff805481bdea27f900c32698171286542b8d3c
        clonedir: edk2/MdePkg/Library/BaseFdtLib/libfdt
    - git|https://github.com/MIPI-Alliance/public-mipi-sys-t.git:
        ref: 370b5944c046bab043dd8b133727b2135af7747a
        clonedir: edk2/MdePkg/Library/MipiSysTLib/mipisyst
    - git|https://github.com/akheron/jansson:
        ref: e9ebfa7e77a6bee77df44e096b100e7131044059
        clonedir: edk2/RedfishPkg/Library/JsonLib/jansson
    - git|https://github.com/DMTF/libspdm.git:
        ref: f55cf6d48ec69b4ac60a63903e9c6a2cb0fd155d
        clonedir: edk2/SecurityPkg/DeviceSecurity/SpdmLib/libspdm
    - git|https://github.com/TrustedComputingGroup/TPM:
        ref: bc29a21d44b01396223c152a4834e52318591770
        clonedir: edk2/TcgTpmPkg/Library/TpmLib/TPM
    - git|https://github.com/tianocore/edk2-cmocka.git:
        ref: 1cc9cde3448cdd2e000886a26acf1caac2db7cf1
        clonedir: edk2/UnitTestFrameworkPkg/Library/CmockaLib/cmocka
    - git|https://github.com/google/googletest.git:
        ref: 86add13493e5c881d7e4ba77fb91c1f57752b3a4
        clonedir: edk2/UnitTestFrameworkPkg/Library/GoogleTestLib/googletest
    - git|https://github.com/tianocore/edk2-subhook.git:
        ref: 83d4e1ebef3588fae48b69a7352cc21801cb70bc
        clonedir: edk2/UnitTestFrameworkPkg/Library/SubhookLib/subhook
homepage    : https://github.com/tianocore/tianocore.github.io/wiki/EDK-II
license     :
    - BSD-2-Clause
    - BSD-3-Clause
    - MIT
    - Python-2.0
    - Zlib
summary     : UEFI support for Virtual Machines
description : |
    OVMF is an EDK II based project to enable UEFI support for Virtual Machines. OVMF contains UEFI firmware for QEMU and KVM.
builddeps   :
    - binary(dd)
    - binary(iasl)
    - binary(make)
    - binary(nasm)
    - binary(python3)
    - pkgconfig(uuid)
    - brotli-devel
environment : |
    _BUILD_ARCHS=(AARCH64 RISCV64 X64)

    # Options that should be used by every build, including the shells
    _CC_OPTS=(
        -b "RELEASE"
        -n "%(jobs)"
        -t "CLANGDWARF"
    )

    # Options that should be used for all EFI binaries
    _EFI_OPTS=(
        -D FD_SIZE_4MB
        -D NETWORK_HTTP_BOOT_ENABLE
        -D NETWORK_IP6_ENABLE
        -D TPM1_ENABLE
        -D TPM2_ENABLE
        -D TPM2_CONFIG_ENABLE
    )

    # network TLS for some targets (all but )
    _NETWORK_TLS=(
        -D NETWORK_TLS_ENABLE
    )

    # The build directly sources scripts
    set +o nounset

    # Sanitize build environment
    unset CFLAGS CXXFLAGS LDFLAGS
build       : |
    # -Werror, not even once
    sed -e 's/ -Werror//g' -i BaseTools/Conf/*.template BaseTools/Source/C/Makefiles/*.makefile

    git clone \
        BaseTools/Source/C/BrotliCompress/brotli \
        MdeModulePkg/Library/BrotliCustomDecompressLib/brotli

    make -j%(jobs) -C BaseTools

    . edksetup.sh

    # Shell
    for _ARCH in ${_BUILD_ARCHS[@]}; do
        # Shell not currently used
        # echo "Building shell for ${_ARCH}"
        # BaseTools/BinWrappers/PosixLike/build \
        #     -p ShellPkg/ShellPkg.dsc \
        #     -a "${_ARCH}" \
        #     ${_CC_OPTS[@]}

        case "${_ARCH}" in
            AARCH64)
                echo "Building ArmVirtPkg ($_ARCH) with secure boot support"
                _build_options=(
                    -p ArmVirtPkg/ArmVirtQemu.dsc
                    -a "$_ARCH"
                    "${_CC_OPTS[@]}"
                    "${_EFI_OPTS[@]}"
                    "${_NETWORK_TLS[@]}"
                    -D SECURE_BOOT_ENABLE
                )
                BaseTools/BinWrappers/PosixLike/build "${_build_options[@]}"
                dd if=/dev/zero of=Build/ArmVirtQemu-AArch64/RELEASE_CLANGDWARF/FV/QEMU_CODE.fd bs=1M count=64
                dd if=Build/ArmVirtQemu-AArch64/RELEASE_CLANGDWARF/FV/QEMU_EFI.fd of=Build/ArmVirtQemu-AArch64/RELEASE_CLANGDWARF/FV/QEMU_CODE.fd conv=notrunc
                dd if=/dev/zero of=Build/ArmVirtQemu-AArch64/RELEASE_CLANGDWARF/FV/QEMU_VARS.fd bs=1M count=64
            ;;

            RISCV64)
                # Does not yet appear to support secure boot
                echo "Building RiscVVirtPkg ($_ARCH)"
                _build_options=(
                    -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc
                    -a "$_ARCH"
                    "${_CC_OPTS[@]}"
                    "${_EFI_OPTS[@]}"
                    "${_NETWORK_TLS[@]}"
                )
                BaseTools/BinWrappers/PosixLike/build "${_build_options[@]}"
                # Create tmp files so we don't overwrite things
                dd if=/dev/zero bs=1M count=32 | tee Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_CODE.fd.tmp Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_VARS.fd.tmp
                dd if=Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_CODE.fd of=Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_CODE.fd.tmp conv=notrunc
                dd if=Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_VARS.fd of=Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_VARS.fd.tmp conv=notrunc
                mv -v Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_CODE.fd{.tmp,}
                mv -v Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/RISCV_VIRT_VARS.fd{.tmp,}
            ;;

            X64)
                echo "Building ovmf ($_ARCH) with microvm support (4MB FD)"
                _build_options=(
                    -p OvmfPkg/Microvm/Microvm$_ARCH.dsc
                    -a "$_ARCH"
                    "${_CC_OPTS[@]}"
                    "${_EFI_OPTS[@]}"
                    "${_NETWORK_TLS[@]}"
                )
                BaseTools/BinWrappers/PosixLike/build "${_build_options[@]}"
                mv -v Build/MicrovmX64{,-4mb}

                echo "Building ovmf ($_ARCH) with secure boot support (4MB FD)"
                _build_options=(
                    -p OvmfPkg/OvmfPkgIa32X64.dsc
                    -a IA32
                    -a "${_ARCH}"
                    "${_CC_OPTS[@]}"
                    "${_EFI_OPTS[@]}"
                    "${_NETWORK_TLS[@]}"
                    -D SECURE_BOOT_ENABLE
                    -D SMM_REQUIRE
                )
                BaseTools/BinWrappers/PosixLike/build "${_build_options[@]}"
                mv -v Build/Ovmf3264{,-secure-4mb}

                echo "Building ovmf (${_ARCH}) without secure boot (4MB FD)"
                _build_options=(
                    -p OvmfPkg/OvmfPkg$_ARCH.dsc
                    -a "$_ARCH"
                    "${_CC_OPTS[@]}"
                    "${_EFI_OPTS[@]}"
                    "${_NETWORK_TLS[@]}"
                )
                BaseTools/BinWrappers/PosixLike/build "${_build_options[@]}"
                mv -v Build/OvmfX64{,-4mb}
            ;;
        esac
    done
install     : |
    # QEMU descriptors
    %install_file %(pkgdir)/*.json -t %(installroot)/usr/share/qemu/firmware

    # AARCH64
    %install_file Build/ArmVirtQemu-AArch64/RELEASE_CLANGDWARF/FV/*.fd -t %(installroot)/usr/share/%(name)/aarch64/

    # RISCV64
    %install_file Build/RiscVVirtQemu/RELEASE_CLANGDWARF/FV/*.fd -t %(installroot)/usr/share/%(name)/riscv64/

    # X64
    %install_file Build/OvmfX64-4mb/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd %(installroot)/usr/share/%(name)/x64/OVMF_CODE.4m.fd
    %install_file Build/OvmfX64-4mb/RELEASE_CLANGDWARF/FV/OVMF_VARS.fd %(installroot)/usr/share/%(name)/x64/OVMF_VARS.4m.fd

    # Secure boot/microvm
    %install_file Build/Ovmf3264-secure-4mb/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd %(installroot)/usr/share/%(name)/x64/OVMF_CODE.secboot.4m.fd
    %install_file Build/MicrovmX64-4mb/RELEASE_CLANGDWARF/FV/MICROVM.fd %(installroot)/usr/share/%(name)/x64/MICROVM.4m.fd

    # This is only used by xen which we don't currently support
    # %install_file Build/OvmfX64-4mb/RELEASE_CLANGDWARF/FV/OVMF.fd %(installroot)/usr/share/%(name)/x64/OVMF.4m.fd
packages    :
    - "%(name)-aarch64":
        summary: Firmware for Virtual Machines (Aarch64)
        description: Firmware for Virtual Machines (Aarch64)
        paths:
            - /usr/share/edk2/aarch64/
            - /usr/share/qemu/firmware/*aarch64*.json

    - "%(name)-riscv64":
        summary: Firmware for Virtual Machines (riscv64)
        description: Firmware for Virtual Machines (riscv64)
        paths:
            - /usr/share/edk2/riscv64/
            - /usr/share/qemu/firmware/*riscv64*.json

    - "%(name)-ovmf":
        summary: Firmware for Virtual Machines (x86_64)
        description: Firmware for Virtual Machines (x86_64)
        paths:
            - /usr/share/edk2/x64/
            - /usr/share/qemu/firmware/*x86_64*.json
            - /usr/share/qemu/firmware/*microvm*.json

```

</details>
