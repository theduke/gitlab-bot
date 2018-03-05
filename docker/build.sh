#! /bin/sh

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

cd $DIR/..
docker build -f docker/Dockerfile -t theduke/gitlab-bot .

