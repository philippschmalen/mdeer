# Native Windows exe, unsigned, distributed via winget

Supersedes [0004](0004-linux-only-windows-via-wsl.md). WSLg has no real window management (no Snap, no proper Alt-Tab or taskbar), so MDeer ships a native `mdeer.exe` again next to the Linux binary.

winget is the Windows install route (`PhilippSchmalen.MDeer`, portable). Microsoft validates and scans its packages before listing them. A release job opens the winget-pkgs PR for each tag.

The exe is unsigned; we won't pay for a certificate. Windows judges an unsigned exe by its reputation, and every release is a new, unknown file. SmartScreen warns about browser downloads. Where Defender's attack surface reduction rule "Block executable files from running unless they meet a prevalence, age, or trusted list criterion" is on, the exe is blocked however it was installed. Signing is not a documented way past that rule. On such devices an administrator has to allow MDeer, or people keep using the WSL route from 0004, which still works. Free signing through SignPath Foundation stays an option.
