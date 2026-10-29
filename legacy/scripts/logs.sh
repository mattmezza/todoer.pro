#!/bin/bash

OPTIND=1

ENV="dev"
TAG=$(git rev-parse --short HEAD)
while getopts "h?e:t:" opt; do
  case "$opt" in
    h|\?)
      echo "Usage: ./scripts/logs.sh -eprod (optional opt and args as per docker compose logs cmd)"
      exit 0
      ;;
    e)  ENV=$OPTARG
      ;;
    t)  TAG=$OPTARG
      ;;
  esac
done
PRJ="todoer_$ENV"
shift $((OPTIND-1))
[ "${1:-}" = "--" ] && shift

# source secrets
source deploy/secrets.env
TAG=$TAG docker compose -p $PRJ -f "deploy/docker-compose.$ENV.yml" logs $@
