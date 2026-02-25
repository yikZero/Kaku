#!/usr/bin/env bash
# Smoke tests for the cleanup_legacy_inline_block awk logic in setup_zsh.sh.
# The awk script strips the legacy inline Kaku block from .zshrc. It returns
# exit code 42 when the block marker is found but not properly terminated.

set -euo pipefail

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_file_eq() {
  local expected_file="$1"
  local actual_file="$2"
  local label="$3"
  if ! cmp -s "$expected_file" "$actual_file"; then
    echo "Expected:" >&2
    cat "$expected_file" >&2
    echo "Actual:" >&2
    cat "$actual_file" >&2
    fail "$label"
  fi
}

# Inline the awk script so the test does not depend on sourcing setup_zsh.sh.
run_cleanup() {
  local input_file="$1"
  local output_file="$2"

  awk '
BEGIN { in_block = 0; saw_syntax = 0; saw_kaku_var = 0 }
{
  if (!in_block && $0 == "# Kaku Shell Integration") {
    in_block = 1
    saw_syntax = 0
    saw_kaku_var = 0
    next
  }

  if (in_block) {
    if ($0 ~ /KAKU_ZSH_DIR/) {
      saw_kaku_var = 1
      next
    }

    if ($0 ~ /zsh-syntax-highlighting\/zsh-syntax-highlighting\.zsh/) {
      saw_syntax = 1
      next
    }

    if (saw_kaku_var && saw_syntax && $0 ~ /^[[:space:]]*fi[[:space:]]*$/) {
      in_block = 0
      saw_syntax = 0
      saw_kaku_var = 0
      next
    }

    next
  }

  print
}
END {
  if (in_block) {
    exit 42
  }
}
' "$input_file" >"$output_file"
}

run_test() {
  local input_text="$1"
  local expected_status="$2"
  local expected_output="$3"
  local label="$4"

  local tmp_dir input_file output_file expected_file status
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/kaku-cleanup-test.XXXXXX")"
  input_file="$tmp_dir/input.zshrc"
  output_file="$tmp_dir/output.zshrc"
  expected_file="$tmp_dir/expected.zshrc"
  printf '%s' "$input_text" >"$input_file"
  printf '%s' "$expected_output" >"$expected_file"

  if run_cleanup "$input_file" "$output_file"; then
    status=0
  else
    status=$?
  fi
  [[ "$status" == "$expected_status" ]] || fail "$label: status expected $expected_status got $status"
  assert_file_eq "$expected_file" "$output_file" "$label"
  rm -rf "$tmp_dir"
}

LEGACY_BLOCK='# Kaku Shell Integration
export KAKU_ZSH_DIR="$HOME/.config/kaku/zsh"
source ~/something/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
fi'

# Test 1: full legacy block is stripped, surrounding lines preserved.
run_test \
  $'export PATH="$HOME/bin:$PATH"\n'"$LEGACY_BLOCK"$'\nexport FOO=bar\n' \
  0 \
  $'export PATH="$HOME/bin:$PATH"\nexport FOO=bar\n' \
  "legacy block is removed and surrounding lines preserved"

# Test 2: no legacy block present - file is passed through unchanged.
run_test \
  $'export PATH="$HOME/bin:$PATH"\n# no kaku block here\n' \
  0 \
  $'export PATH="$HOME/bin:$PATH"\n# no kaku block here\n' \
  "no legacy block passes through unchanged"

# Test 3: unterminated block (missing closing fi) returns exit code 42.
run_test \
  $'# Kaku Shell Integration\nexport KAKU_ZSH_DIR="$HOME/.config/kaku/zsh"\n' \
  42 \
  "" \
  "unterminated block exits 42 and produces no output"

echo "cleanup_legacy_inline_block smoke tests passed"
