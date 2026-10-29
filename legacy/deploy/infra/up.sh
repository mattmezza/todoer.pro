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
                client_max_body_size 20M;
                # Uncomment and add IPs to restrict access:
                # allow 127.0.0.1;
                # allow YOUR.IP.HERE;
                # deny all;
        }
}' | sudo tee /etc/nginx/sites-available/dev.todoer.pro > /dev/null
sudo ln -s /etc/nginx/sites-available/dev.todoer.pro /etc/nginx/sites-enabled/

# nginx proxypass prod
echo 'server {
        listen 80;
        listen [::]:80;
        server_name todoer.pro;
        location / {
                proxy_pass http://localhost:4010/;
                proxy_buffering off;
                proxy_set_header   Host             $host;
                proxy_set_header   X-Real-IP        $remote_addr;
                proxy_set_header   X-Forwarded-For  $proxy_add_x_forwarded_for;
                client_max_body_size 20M;
        }
}' | sudo tee /etc/nginx/sites-available/todoer.pro > /dev/null
sudo ln -s /etc/nginx/sites-available/todoer.pro /etc/nginx/sites-enabled/

sudo systemctl reload nginx

echo "Running certbot..."
echo "At this point, the DNS should be already configured (CHECK THAT PLS)"
read -p "Press enter to continue"
sudo certbot --nginx
