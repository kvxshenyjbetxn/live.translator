#!/bin/bash
# Succeeds only for an ad-hoc signed app. Kept as its own script so the
# regression test can run this exact check against a stub codesign binary.
set -euo pipefail

app="${1:?usage: verify-macos-signature.sh <app> [codesign]}"
codesign="${2:-/usr/bin/codesign}"

# Read the whole report before matching. `codesign ... | grep -q` closes the pipe
# at the first match, so codesign keeps writing to a dead pipe, dies with SIGPIPE
# (141) and, under pipefail, aborts packaging even though the signature is fine.
display="$("$codesign" --display --verbose=2 "$app" 2>&1)" || {
  printf '%s\n' "$display" >&2
  echo "codesign --display failed for $app" >&2
  exit 1
}
if ! grep -q '^Signature=adhoc$' <<<"$display"; then
  printf '%s\n' "$display" >&2
  echo "Expected an ad-hoc signature (Signature=adhoc) on $app" >&2
  exit 1
fi
