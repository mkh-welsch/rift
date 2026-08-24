#!/usr/bin/env bash
set -euo pipefail

filesystem="${1:?usage: linux-fs.sh <btrfs|xfs-reflink|ext4>}"
workspace="${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}"
runner_temp="${RUNNER_TEMP:?RUNNER_TEMP is required}"
mountpoint="/mnt/greppy-cow-${filesystem}"
image="${runner_temp}/greppy-cow-${filesystem}.img"
loop_device=""

cleanup() {
  local status=$?
  set +e
  if mountpoint -q "${mountpoint}"; then
    sudo umount "${mountpoint}"
  fi
  if [[ -n "${loop_device}" ]]; then
    sudo losetup --detach "${loop_device}"
  fi
  sudo rmdir "${mountpoint}" 2>/dev/null || true
  exit "${status}"
}
trap cleanup EXIT

sudo mkdir -p "${mountpoint}"
truncate -s 1G "${image}"

case "${filesystem}" in
  btrfs)
    mkfs.btrfs -f "${image}"
    expected_backend="btrfs_snapshot"
    ;;
  xfs-reflink)
    mkfs.xfs -f -m reflink=1 "${image}"
    expected_backend="linux_reflink_tree"
    ;;
  ext4)
    mkfs.ext4 -F "${image}"
    expected_backend="unavailable"
    ;;
  *)
    echo "unsupported fixture: ${filesystem}" >&2
    exit 2
    ;;
esac

loop_device="$(sudo losetup --find --show "${image}")"
sudo mount "${loop_device}" "${mountpoint}"
sudo chown "${USER}:${USER}" "${mountpoint}"

source="${mountpoint}/source"
destination_root="${mountpoint}/snapshots"
mkdir -p "${destination_root}"
if [[ "${filesystem}" == "btrfs" ]]; then
  btrfs subvolume create "${source}"
else
  mkdir "${source}"
fi
mkdir "${source}/nested"
printf 'source\n' >"${source}/nested/file.txt"

GREPPY_COW_TEST_SOURCE="${source}" \
GREPPY_COW_TEST_DESTINATION_ROOT="${destination_root}" \
GREPPY_COW_EXPECT_BACKEND="${expected_backend}" \
  cargo test \
    --manifest-path "${workspace}/Cargo.toml" \
    --package greppy-rift-core \
    --test filesystem \
    --locked \
    -- --nocapture
