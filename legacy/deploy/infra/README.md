infra
===

Two scripts:

- `install.sh`: this is to install all the dependencies that this app needs
  - `nginx`
  - `certbot`
- `up.sh`: to bring up all the dependencies needed by this app to work
  - nxing reverse proxies
  - ssl certificates via letsencrypt


# usage

Make sure the dns to which you want to deploy points already to the server ip.

- `sudo ./deploy/infra/install.sh`
- `sudo ./deploy/infra/up.sh`
