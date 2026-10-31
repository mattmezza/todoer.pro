#!/bin/sh
# Certbot deploy hook for the app.todoer.pro webroot certificate. Adjust the
# lineage when using a different certificate name.
set -eu
case "${RENEWED_LINEAGE:-}" in
    /etc/letsencrypt/live/app.todoer.pro) ;;
    *) exit 0 ;;
esac
nginx -t
systemctl reload nginx
