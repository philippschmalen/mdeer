#!/bin/sh
# Installs MDeer for the current user (no sudo), on Ubuntu or in WSL2 on Windows 11:
#   curl -fsSL https://raw.githubusercontent.com/philippschmalen/mdeer/main/install.sh | sh
set -eu

[ "$(uname -m)" = x86_64 ] || { echo "mdeer: only x86_64 builds are published; build from source instead" >&2; exit 1; }

bin="$HOME/.local/bin"
mkdir -p "$bin"
curl -fsSL -o "$bin/mdeer.tmp" https://github.com/philippschmalen/mdeer/releases/latest/download/mdeer-x86_64-linux
chmod +x "$bin/mdeer.tmp"
mv "$bin/mdeer.tmp" "$bin/mdeer"
"$bin/mdeer" --register

# Loaded at runtime (not linked), so a missing one only shows up when the window opens.
missing=""
for lib in libEGL.so.1 libxkbcommon.so.0 libwayland-client.so.0 libwayland-cursor.so.0; do
    ldconfig -p | grep -q "$lib" || missing="$missing $lib"
done
if [ -n "$missing" ]; then
    echo "Missing libraries:$missing"
    echo "  sudo apt install libegl1 libxkbcommon0 libwayland-client0 libwayland-cursor0"
fi

case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "Add $bin to your PATH to run 'mdeer' from a terminal." ;;
esac
echo "Installed MDeer to $bin/mdeer"
