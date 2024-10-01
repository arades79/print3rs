# Print3rs
### A rusty kind of 3D printer host toolkit

The goal of this repo is to provide functionality on par with, and eventually exceeding the popular python toolkit [Printrun](https://www.pronterface.com/).

Initially, this means:
* A Rust library to ease building a 3D printer host application
* A cross-platform cli/console utility to talk to a 3D printer
* A cross-platform GUI with customizable UI to interact with 3D printers

Eventually, we would like to implement:
* Unified tooling to talk over USB/Serial, Wifi/TCP, Bluetooth, or anything else!
* g-code slicing
* An embedded (through browser) Printer UI
* More complex print staging
* Non 3D-printer CNC machine integration

## !!! Under Active development !!!
Any interfaces in any of the crates are subject to radical breaking changes without notice.
User binaries could have very different semantics from commit to commit.

Until a reasonable 0.1 is met, don't use anything in this repo in other projects!

In the mean time, testing using a tagged release, any code reviewing, or contributions are accepted :D

## Licencing
All _library_ code is permissively licensed under MIT, making it compatible with almost any codebase, and a no-brainer to bring in from Cargo where needed

All _application_ code is licenced under GPLv3, so if you want to use the existing console or GUI directly, you will have to adopt GPL. 

All _documentation_ or non-code related artifacts are Public Domain, unless otherwise specified.

This gives flexibility for anyone to build their own hosts based on the libraries, but if you want to skip that work,
we ask that you keep those changes open and share your improvements.

## Building

The fastest way to get started with development/building is to use the devcontainer. This will include all dependencies needed for building any of the crates in the workspace.

The only dependency required for the library crates is libudev-dev or some other package to privide udev headers. In some distros this is provided through systemd development packages.

No dependencies are required for Windows development, apart from the msvc compiler toolchain and headers you would normally need.

Absolutely no attempt has been made for MacOS compatibility, but as everything used is cross-platform, it probably works. If someone can verify that MacOS works, I can setup releases to build unsigned versions.