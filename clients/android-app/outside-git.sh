# Sourced by sign-release.sh and d12-binding.sh (#434 D12), which define
# `fail`. outside_git PATH fails when PATH, or the nearest folder above it
# that exists, is inside a Git repository: there `git add` could stage a key
# that lives in PATH. It also fails when git is missing or cannot answer
# (for example a repository that git calls of dubious ownership), so the
# check never passes because it could not run. GIT_DIR and GIT_WORK_TREE,
# which would point git elsewhere, are ignored, and git answers in English.
# shellcheck shell=bash
outside_git() {
  local path answer
  command -v git > /dev/null || fail "git is missing, so the check for a Git work tree around $1 cannot run"
  path="$(realpath -m "$1")"
  while [[ ! -e "$path" ]]; do path="$(dirname "$path")"; done
  if answer="$(env -u GIT_DIR -u GIT_WORK_TREE LC_ALL=C git -C "$path" rev-parse --show-toplevel 2>&1)"; then
    fail "$1 is inside the Git work tree $answer; keep it outside every checkout"
  fi
  [[ "$answer" == *"not a git repository"* ]] ||
    fail "git cannot tell whether $1 is inside a Git work tree: $answer"
}
