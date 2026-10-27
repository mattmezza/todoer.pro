# Todoer public website

Static website for Cloudflare Pages. Configure the Pages project with `www` as the build output directory and no build command. The site links to the separately hosted app at `https://app.todoer.pro`.

Run `make www-dev` from the repository root, or `make www-dev port=8001` to choose a port. The server listens on loopback only.
