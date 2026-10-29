# Linux-only build; Windows 11 runs MDeer through WSL2

Superseded by [0005](0005-native-windows-unsigned-via-winget.md).

We dropped the native `mdeer.exe`. Windows can refuse to run an unsigned exe that has no reputation yet, and code signing needs an identity and reputation we don't have. MDeer now ships one Linux x86_64 binary (built on Ubuntu 24.04, so it needs glibc 2.39+) and runs on Windows 11 inside WSL2, with its window shown through WSLg.

Under WSL, `mdeer --register` adds MDeer to Explorer's "Open with" for `.md`. The command is `conhost.exe --headless wsl.exe -d <distro> -e <mdeer> "%1"`: only Microsoft-signed System32 binaries, and no console window. `wslg.exe` would be the obvious launcher but exited with -1 in our tests. MDeer maps the Windows paths Explorer passes (`C:\…`, `\\wsl.localhost\<distro>\…`) into the Linux filesystem itself.
