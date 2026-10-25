#!/usr/bin/env bash
# PostToolUse: format, lint and type-check after an agent edits a Rust file.
f=$(jq -r '.tool_input.file_path // empty')
[[ "$f" == *.rs ]] || exit 0
PATH="$HOME/.cargo/bin:$PATH"
cd "$CLAUDE_PROJECT_DIR" || exit 0
rustfmt --edition 2024 "$f" 2>&1 >&2 || exit 2
out=$(cargo clippy --quiet --all-targets --message-format=short -- -D warnings 2>&1) || { echo "$out" >&2; exit 2; }
