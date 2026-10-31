#!/bin/sh
# Certbot deploy hook: install in /etc/letsencrypt/renewal-hooks/deploy/.
set -eu
nginx -t
systemctl reload nginx
