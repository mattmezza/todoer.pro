#!/bin/bash
set -e
trap 'last_command=$current_command; current_command=$BASH_COMMAND' DEBUG
trap 'echo "\"${last_command}\" command filed with exit code $?."' EXIT

# update sources
sudo apt update
sudo apt install make -y  # needed for the deploy

# install nginx
sudo apt install nginx -y
sudo systemctl status nginx
sudo ufw app list
sudo ufw allow 'Nginx Full'
sudo ufw status

# securing with ssl
sudo apt install certbot python3-certbot-nginx -y

# adding automated daily pruning for docker
echo '#!/bin/bash
docker system prune -af  --filter "until=$((30*24))h"' | sudo tee /etc/cron.daily/docker-prune > /dev/null
sudo chmod +x /etc/cron.daily/docker-prune
echo "docker pruning script installed in /etc/cron.daily"
echo "use 'run-parts /etc/cron.daily' to test it"

# install docker compose
DOCKER_CONFIG=${DOCKER_CONFIG:-$HOME/.docker}
mkdir -p $DOCKER_CONFIG/cli-plugins
curl -SL https://github.com/docker/compose/releases/download/v2.4.1/docker-compose-linux-x86_64 -o $DOCKER_CONFIG/cli-plugins/docker-compose
chmod +x $DOCKER_CONFIG/cli-plugins/docker-compose

