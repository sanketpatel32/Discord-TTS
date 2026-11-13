#!/bin/sh
set -e
mkdir -p /app/data
chown -R vcreader:vcreader /app/data
exec gosu vcreader /app/vc-reader "$@"
