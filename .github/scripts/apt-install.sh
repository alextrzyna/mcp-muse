#!/usr/bin/env bash
# Install apt packages on a GitHub Ubuntu runner without letting a broken
# third-party source abort the job.
#
# The runner image ships Google Chrome's apt source. It intermittently fails
# its hash check, which makes `apt-get update` exit 100 even though every
# Ubuntu index was fetched. None of our builds need that source, so drop it,
# tolerate leftover index errors, and let `apt-get install` be the real check.
set -uo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: apt-install.sh PACKAGE..." >&2
  exit 2
fi

sudo rm -f /etc/apt/sources.list.d/google-chrome*.list \
           /etc/apt/sources.list.d/google-chrome*.sources
if ! sudo apt-get update -o Acquire::Retries=3; then
  echo "::warning::apt-get update reported errors; installing with the indexes that were fetched"
fi

set -e
sudo apt-get install -y "$@"
