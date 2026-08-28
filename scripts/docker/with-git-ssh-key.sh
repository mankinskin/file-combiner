#!/usr/bin/env bash
# Enables the SSH deploy key for the private `agent-builder` git dependency
# when Render's Secret File mount is present, then runs the given command.
# Local/CI builds instead forward a live ssh-agent (docker build --ssh
# default), so this is a no-op there. Usage: with-git-ssh-key.sh <cmd> [args...]
set -uo pipefail

SECRET_FILE=/etc/secrets/agent_builder_ssh_key

if [ -f "$SECRET_FILE" ]; then
  echo "with-git-ssh-key: secret file present at $SECRET_FILE ($(wc -c < "$SECRET_FILE") bytes)" >&2
  # Some secret-mount implementations grant permissions ssh's strict mode
  # rejects (e.g. group/world-readable); copy to a key ssh always trusts.
  KEY_FILE=/root/.ssh/agent_builder_deploy_key
  install -m 600 "$SECRET_FILE" "$KEY_FILE"
  export GIT_SSH_COMMAND="ssh -i $KEY_FILE -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"

  # Fails loudly with the real git/ssh error here, rather than surfacing
  # only as a generic downstream `cargo` exit code with no context.
  echo "with-git-ssh-key: verifying access to agent-builder via SSH..." >&2
  if ! git ls-remote ssh://git@github.com/mankinskin/agent-builder HEAD >&2; then
    echo "with-git-ssh-key: FATAL: could not reach/authenticate to" \
         "ssh://git@github.com/mankinskin/agent-builder using $SECRET_FILE." \
         "Check the Secret File's name/contents, and that the deploy key" \
         "is whitelisted on that repo." >&2
    exit 1
  fi
  echo "with-git-ssh-key: SSH access to agent-builder confirmed." >&2
else
  echo "with-git-ssh-key: no secret file at $SECRET_FILE; relying on a forwarded ssh-agent instead." >&2
fi

# Ordinary failures already stream live via the pipe below; this capture
# exists only so a failure can be followed by an unmissable, explicitly
# bounded replay of its tail — in case a log viewer truncates or buffers
# very long/mixed stdout+stderr output and hides the actual error line.
echo "with-git-ssh-key: running: $*" >&2
LOG_FILE=$(mktemp)
"$@" 2>&1 | tee "$LOG_FILE"
status=${PIPESTATUS[0]}

if [ "$status" -ne 0 ]; then
  echo "" >&2
  echo "===== with-git-ssh-key: '$*' failed (exit $status) =====" >&2
  echo "----- last 200 lines of its output (see above for full log) -----" >&2
  tail -n 200 "$LOG_FILE" >&2
  echo "===== end of failure replay =====" >&2
fi

rm -f "$LOG_FILE"
exit "$status"
