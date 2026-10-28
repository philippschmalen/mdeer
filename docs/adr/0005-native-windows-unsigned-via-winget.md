# Native Windows exe, unsigned, distributed via winget

Supersedes [0004](0004-linux-only-windows-via-wsl.md). WSLg has no real window management (no Snap, no proper Alt-Tab or taskbar), so MDeer ships a native `mdeer.exe` again next to the Linux binary.

An unsigned exe downloaded in a browser carries the Mark of the Web, and Microsoft Defender SmartScreen shows an "Unknown publisher" prompt before it runs. winget is the supported install route instead (`PhilippSchmalen.MDeer`, portable): its packages are validated and scanned by Microsoft before they are listed. A release job opens the winget-pkgs PR for each tag.

We won't pay for a signing certificate; it mainly helps SmartScreen on browser downloads. Free signing through SignPath Foundation (publisher shown as "SignPath Foundation") stays an option if browser downloads matter later. If Explorer launches of a winget-installed copy still show the prompt, the README documents a one-time `Unblock-File` rather than having MDeer strip its own mark.
