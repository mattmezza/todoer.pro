#!/bin/bash
set -e

OPTIND=1

ENV="dev"
TAG=$(git rev-parse --short HEAD)
while getopts "h?e:t:" opt; do
  case "$opt" in
    h|\?)
      echo "Usage: ./scripts/migrate.sh -eprod migrations/0001.up.sql"
      exit 0
      ;;
    e)  ENV=$OPTARG
      ;;
    t)  TAG=$OPTARG
      ;;
  esac
done
shift $((OPTIND-1))
[ "${1:-}" = "--" ] && shift

if [[ "$1" == *.py ]]; then
    echo "running python script via docker..."
    echo "building docker image..."
    docker build -t migration:$TAG - < scripts/migration.Dockerfile
    echo "image built! Running 'migration:$TAG' via docker..."
    docker run -it \
        -v "$(realpath $1):/app/$(basename $1)" \
        -v "$(pwd)/deploy/data/todoer_$ENV.db:/app/db" \
        migration:$TAG python "$(basename $1)" "sqlite:////app/db"
    echo "$1 run successfully via docker!"
else
    echo "running sql script..."
    cat "$1" | sqlite3 "deploy/data/todoer_$ENV.db"
    echo "$1 run successfully!"
fi
