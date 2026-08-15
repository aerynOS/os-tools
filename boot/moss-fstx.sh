#!/bin/sh
# SPDX-FileCopyrightText: 2025 AerynOS Developers
# SPDX-License-Identifier: MPL-2.0

type getarg > /dev/null 2>&1 || . /lib/dracut-lib.sh
command -v moss > /dev/null || exit 1

[ -z "$1" ] && exit 1
sysroot="$1"

# Grab the moss.fstx ID
fstx_id=$(getarg moss.fstx)
[ -z "$fstx_id" ] && exit 0

# Grab the current fstx from the static copy stored under the moss root
current_fstx=$(cat "$sysroot/.moss/root/overlayimg/.stateID" 2>/dev/null)
[ -z "$current_fstx" ] && exit 1

# Activate the requested fstx (no-op if already active & mounted).
#
# TODO: Ask the user if they want to perform the rollback using plymouth.
moss -D "$sysroot" state activate -y --skip-triggers "$fstx_id"
