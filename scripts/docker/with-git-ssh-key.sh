#!/usr/bin/env bash
# Enables the SSH deploy key for the private `agent-builder` git dependency
# when Render's Secret File mount is present, then execs the given command.
# Local/CI builds instead forward a live ssh-agent (docker build --ssh
# default), so this is a no-op there. Usage: with-git-ssh-key.sh <cmd> [args...]
set -euo pipefail

if [ -f /etc/secrets/agent_builder_ssh_key ]; then
  export GIT_SSH_COMMAND="ssh -i /etc/secrets/agent_builder_ssh_key -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"
fi

exec "$@"
