# `ogaki`

> "ogaki" (御幾) that referred to notice boards or public signboards in ancient
> Japan. Ogaki served as an important means of communication and spreading
> information to the common people before newspapers and mass media existed.

## CLI Reference

``` sh
ogaki --help
```

```
Utility for automatic update-n-start processes of YUVd binaries.


Usage: ogaki <COMMAND>

Commands:
  update                Check for yuvd updates and install them
  check-updates         Check for yuvd updates
  run-with-auto-update  Run yuvd, automatically checking for updates and installing them
  help                  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Build container with `ogaki` and `yuvd`

Checkout the infrastructure's **Build** section at
[README](../../infrastructure/README.md) to locally build fully functional
upgredable node.
