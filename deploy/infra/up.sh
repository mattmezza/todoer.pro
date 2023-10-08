#!/bin/bash
set -e
trap 'last_command=$current_command; current_command=$BASH_COMMAND' DEBUG
trap 'echo "\"${last_command}\" command filed with exit code $?."' EXIT

# nginx proxypass dev
echo "nginx installed: server ip is '$(hostname -I)'."
echo 'server {
        listen 80;
        listen [::]:80;
        server_name dev.todoer.pro;
        location / {
                proxy_pass http://localhost:4001/;
                proxy_buffering off;
                proxy_set_header   Host             $host;
                proxy_set_header   X-Real-IP        $remote_addr;
                proxy_set_header   X-Forwarded-For  $proxy_add_x_forwarded_for;
                allow 127.0.0.1;
                allow 172.20.0.0/16;
                allow 172.21.0.0/16;
                allow 172.22.0.0/16;
                allow 86.84.57.38;
                ######################
                # allow stripe webhooks
                # https://stripe.com/files/ips/ips_webhooks.txt
                # https://stripe.com/docs/ips
                allow 3.18.12.63;
                allow 3.130.192.231;
                allow 13.235.14.237;
                allow 13.235.122.149;
                allow 18.211.135.69;
                allow 35.154.171.200;
                allow 52.15.183.38;
                allow 54.88.130.119;
                allow 54.88.130.237;
                allow 54.187.174.169;
                allow 54.187.205.235;
                allow 54.187.216.72;
                # end list of stripe webhooks
                ######################
               deny all;
        }
}' | sudo tee /etc/nginx/sites-available/dev.todoer.pro > /dev/null
sudo ln -s /etc/nginx/sites-available/dev.todoer.pro /etc/nginx/sites-enabled/

# nginx proxypass prod
echo 'server {
        listen 80;
        listen [::]:80;
        server_name zenleadr.com;
        location / {
                proxy_pass http://localhost:4010/;
                proxy_buffering off;
                proxy_set_header   Host             $host;
                proxy_set_header   X-Real-IP        $remote_addr;
                proxy_set_header   X-Forwarded-For  $proxy_add_x_forwarded_for;
        }
}' | sudo tee /etc/nginx/sites-available/todoer.pro > /dev/null
sudo ln -s /etc/nginx/sites-available/todoer.pro /etc/nginx/sites-enabled/

sudo systemctl reload nginx

echo "Running certbot..."
echo "At this point, the DNS should be already configured (CHECK THAT PLS)"
read -p "Press enter to continue"
sudo certbot --nginx
