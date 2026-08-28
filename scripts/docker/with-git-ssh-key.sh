#!/usr/bin/env bash
# Enables the SSH deploy key for the private `agent-builder` git dependency
# when Render's Secret File mount is present, then runs the given command.
# Local/CI builds instead forward a live ssh-agent (docker build --ssh
# default), so this is a no-op there. Usage: with-git-ssh-key.sh <cmd> [args...]
set -uo pipefail

SECRET_FILE=/etc/secrets/agent_builder_ssh_key

if [ -f "$SECRET_FILE" ]; then
  echo "with-git-ssh-key: secret file present at $SECRET_FILE ($(wc -c < "$SECRET_FILE") bytes, $(wc -l < "$SECRET_FILE") lines)" >&2
  # Some secret-mount implementations grant permissions ssh's strict mode
  # rejects (e.g. group/world-readable); copy to a key ssh always trusts.
  # Also normalize CRLF line endings and guarantee exactly one trailing
  # newline: an OpenSSH/PEM key with either wrong produces the opaque
  # "error in libcrypto" from ssh, which gives no hint the *content*,
  # rather than the credential itself, is at fault.
  KEY_FILE=/root/.ssh/agent_builder_deploy_key
  tr -d '\r' < "$SECRET_FILE" > "$KEY_FILE"
  if [ -n "$(tail -c1 "$KEY_FILE")" ]; then
    echo >> "$KEY_FILE"
  fi
  chmod 600 "$KEY_FILE"
  echo "with-git-ssh-key: normalized key spans $(head -n1 "$KEY_FILE") .. $(tail -n1 "$KEY_FILE")" >&2

  # Validates the key is a parseable private key *before* attempting any
  # network call, so a malformed/truncated secret is never confused with an
  # auth/whitelist failure — each gets its own unambiguous error here.
  if ! ssh-keygen -y -f "$KEY_FILE" > /dev/null 2> /tmp/keycheck.err; then
    echo "with-git-ssh-key: FATAL: $KEY_FILE is not a private key ssh-keygen can parse:" >&2
    cat /tmp/keycheck.err >&2
    echo "with-git-ssh-key: this means the Secret File's *content* is malformed" \
         "(wrong line endings, missing/extra newline, truncation, or it isn't" \
         "actually the private key) — not a whitelisting/permissions problem." >&2
    exit 1
  fi
  echo "with-git-ssh-key: key parses OK, fingerprint: $(ssh-keygen -lf "$KEY_FILE")" >&2

  export GIT_SSH_COMMAND="ssh -i $KEY_FILE -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"

  # Fails loudly with the real git/ssh error here, rather than surfacing
  # only as a generic downstream `cargo` exit code with no context.
  echo "with-git-ssh-key: verifying access to agent-builder via SSH..." >&2
  if ! git ls-remote ssh://git@github.com/mankinskin/agent-builder HEAD >&2; then
    echo "with-git-ssh-key: FATAL: could not reach/authenticate to" \
         "ssh://git@github.com/mankinskin/agent-builder using $SECRET_FILE." \
         "The key parses fine, so check that its *public* half (fingerprint" \
         "above) is whitelisted as a deploy key on that repo." >&2
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
