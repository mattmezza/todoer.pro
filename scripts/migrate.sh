#!/bin/bash

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

cat "$1" | sqlite3 "../todoers-$ENV.db"
