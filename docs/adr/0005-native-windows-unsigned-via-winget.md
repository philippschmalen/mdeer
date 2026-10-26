# Native Windows exe, unsigned, distributed via winget

Supersedes [0004](0004-linux-only-windows-via-wsl.md). WSLg has no real window management (no Snap, no proper Alt-Tab or taskbar), so MDeer ships a native `mdeer.exe` again next to the Linux binary.

0004 blamed the block on "AppLocker/WDAC-style" policy. On the managed machine it turned out to be Microsoft Defender SmartScreen ("Unknown publisher", with "Run anyway" disabled by policy). AppLocker and CodeIntegrity logged nothing. SmartScreen checks files that carry the Mark of the Web, which a browser download adds. We distribute through winget (`PhilippSchmalen.MDeer`, portable) instead, which that machine allows. A release job opens the winget-pkgs PR for each tag.

We won't pay for a signing certificate. Signing would not satisfy path- or publisher-based AppLocker rules anyway; it mainly helps SmartScreen on browser downloads. Free signing through SignPath Foundation (publisher shown as "SignPath Foundation") stays an option if browser downloads matter later. winget also marks its downloads, so if Explorer launches still hit SmartScreen, the README documents a one-time `Unblock-File` rather than having MDeer strip its own mark.
