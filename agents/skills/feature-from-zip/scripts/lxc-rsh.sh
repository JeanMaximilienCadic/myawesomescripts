#!/usr/bin/env bash
# rsync remote-shell shim: rsync calls `<rsh> <host> rsync --server ...`; drop the fake host
# and run the rsync server inside the LXD container (as foo) through the LAN host.
shift
exec ssh -o ConnectTimeout=10 "$FFZ_HOST" "$FFZ_LXC" exec "$FFZ_CT" -- runuser -u foo -- "$@"
