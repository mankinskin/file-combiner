#!/usr/bin/env bash
# Enables the SSH deploy key for the private `agent-builder` git dependency
# when Render's Secret File mount is present, then execs the given command.
# Local/CI builds instead forward a live ssh-agent (docker build --ssh
# default), so this is a no-op there. Usage: with-git-ssh-key.sh <cmd> [args...]
set -euo pipefail

SECRET_FILE=/etc/secrets/agent_builder_ssh_key

if [ -f "$SECRET_FILE" ]; then
  # Some secret-mount implementations grant permissions ssh's strict mode
  # rejects (e.g. group/world-readable); copy to a key ssh always trusts.
  KEY_FILE=/root/.ssh/agent_builder_deploy_key
  install -m 600 "$SECRET_FILE" "$KEY_FILE"
  export GIT_SSH_COMMAND="ssh -i $KEY_FILE -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"

  # Fails loudly with the real git/ssh error here, rather than surfacing
  # only as a generic downstream `cargo` exit code with no context.
  echo "with-git-ssh-key: verifying access to agent-builder via SSH..." >&2
  if ! git ls-remote ssh://git@github.com/mankinskin/agent-builder HEAD >/dev/null; then
    echo "with-git-ssh-key: FATAL: could not reach/authenticate to" \
         "ssh://git@github.com/mankinskin/agent-builder using $SECRET_FILE." \
         "Check the Secret File's name/contents, and that the deploy key" \
         "is whitelisted on that repo." >&2
    exit 1
  fi
  echo "with-git-ssh-key: SSH access to agent-builder confirmed." >&2
fi

exec "$@"
