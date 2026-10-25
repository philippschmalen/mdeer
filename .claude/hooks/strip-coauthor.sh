#!/usr/bin/env bash
# PreToolUse(Bash): drop Co-authored-by trailers from `git commit` commands.
cmd=$(jq -r '.tool_input.command // empty')
[[ "$cmd" == *"git commit"* ]] && grep -qi 'co-authored-by:' <<<"$cmd" || exit 0
new=$(sed -E '/^[[:space:]]*co-authored-by:/Id; s/[[:space:]]+-m[[:space:]]+("co-authored-by:[^"]*"|'"'"'co-authored-by:[^'"'"']*'"'"')//Ig' <<<"$cmd")
jq -n --arg c "$new" '{hookSpecificOutput: {hookEventName: "PreToolUse", updatedInput: {command: $c}}}'
