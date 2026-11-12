#!/usr/bin/env bash
# Kept for old command lines: the box profile of gym.sh (the box is for races only from cycle 6).
PROFILE=box-b PIDTAG=${PIDTAG:-main} exec "$(cd "$(dirname "$0")" && pwd)/gym.sh" "$@"
