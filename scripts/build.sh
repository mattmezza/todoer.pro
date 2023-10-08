#!/bin/bash
set -e
trap 'last_command=$current_command; current_command=$BASH_COMMAND' DEBUG
trap 'echo "\"${last_command}\" returned exit code $?."' EXIT

TAG=$(git rev-parse --short HEAD)
ENV=${1-dev}
shift


docker build -t todoers_app:$TAG .
