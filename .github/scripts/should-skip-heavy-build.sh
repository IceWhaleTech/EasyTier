#!/usr/bin/env bash

set -euo pipefail

should_skip=false

if [[ "${EVENT_NAME:-}" == "push" && "${REF_NAME:-}" == icewhale/* ]]; then
  before_sha="${BEFORE_SHA:-}"
  after_sha="${GITHUB_SHA:-}"

  if [[ -z "$after_sha" ]]; then
    echo "GITHUB_SHA is required" >&2
    exit 1
  fi

  if [[ -n "$before_sha" && "$before_sha" != "0000000000000000000000000000000000000000" ]]; then
    mapfile -t changed_files < <(git diff --name-only "$before_sha" "$after_sha")
  else
    mapfile -t changed_files < <(git diff-tree --no-commit-id --name-only -r "$after_sha")
  fi

  if (( ${#changed_files[@]} > 0 )); then
    should_skip=true
    for file in "${changed_files[@]}"; do
      if [[ ! "$file" =~ ^\.github/workflows/[^/]+\.yml$ ]]; then
        should_skip=false
        break
      fi
    done
  fi
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "should_skip=${should_skip}" >> "$GITHUB_OUTPUT"
fi

echo "should_skip=${should_skip}"
