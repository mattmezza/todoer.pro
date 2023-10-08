#!/bin/bash
set -e
trap 'last_command=$current_command; current_command=$BASH_COMMAND' DEBUG
trap 'echo "\"${last_command}\" command returned exit code $?."' EXIT

OPTIND=1

ENV="dev"
TAG=$(git rev-parse --short HEAD)
detached="-d"

ALL_YES="n"
while getopts "h?ae:t:y" opt; do
  case "$opt" in
    h|\?)
      echo "Usage: ./scripts/deploy.sh -a (if you want to attach logs)"
      exit 0
      ;;
    a)  detached=""
      ;;
    e)  ENV=$OPTARG
      ;;
    t)  TAG=$OPTARG
      ;;
    y)  ALL_YES="y"
      ;;
  esac
done

PRJ="todoer_$ENV"

shift $((OPTIND-1))
[ "${1:-}" = "--" ] && shift

# check if images are available
if [[ $(docker images -q todoer_app:$TAG 2> /dev/null) == "" ]]; then
    echo 3
fi

# source secrets
source deploy/secrets.env

CONFIRMED=y
case "$ENV" in
    prod)
        if [[ $ALL_YES == "n" ]]
        then
            read -p "Are you sure you want to deploy $PRJ @ $TAG to env $ENV? [yN] " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]
            then
                CONFIRMED=n
            fi
        fi
        ;;
    *)
esac

if [ $CONFIRMED == "y" ];
then
    # update stack
    TAG=$TAG docker-compose -p $PRJ -f "deploy/docker-compose.$ENV.yml" up --force-recreate --remove-orphans $detached $@
else
    echo "Skipping deploy to $ENV for $PRJ."
fi
