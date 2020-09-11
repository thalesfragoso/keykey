# Keykey

> Runtime configurable keys (USB HID) for the STM32F103 - WIP

### Prerequisites

The best way to setup the toolchain is to use `rustup`, so go ahead and [install it](https://www.rust-lang.org/tools/install). After that you can run:

```console
$ rustup target add thumbv7m-none-eabi
```

If you want to inspect the binary you will also want to run:
```console
$ cargo install cargo-binutils
$ rustup component add llvm-tools-preview
```

This project uses [cargo-make](https://crates.io/crates/cargo-make) to easily deal with the workspace with different targets:

```console
$ cargo install cargo-make
```

This is optional, you can also manually type the commands used in [Makefile.toml](Makefile.toml).

### Compilation

Simply run the following command in the workspace root:

```console
$ cargo make all
```

The resulting binaries can be found in the `target` folder, the firmware will be in `target/thumbv7-none-eabi/release/keykey` and the cli utility in `target/release/keyconfig`.

### Flashing

The easiest way is if you have a debug probe compatible with [cargo-flash](https://crates.io/crates/cargo-flash), then you can just run:

```console
$ cargo install cargo-flash
$ cargo make flash
```

Note that depending on the firmware you have running on the board you will need to hold the reset button in the beginning of the flash stage.
You can also use `objcopy` in the resulting `elf` file to get a `bin` file and use that with a serial bootloader, note that if you are using a custom bootloader and if it lives in the normal program space it will be overwritten.
There are also `.gdb` and `.cfg` files in the firmware folder to be used with `openocd` and `gdb`.

### Connections

PA0 to PA2 -> Active-low inputs with internal pull-ups and software debouncing.

### CLI usage

The CLI is self explanatory, you can type to search for the key you want in the key selection screen. Vertical scrolling is not implemented yet, you can use the search to reduce the amount of selectable keys on the screen.

You will need to properly configure your `udev` rules to be able to send features reports to the device.

VID: 0x1209 PID: 0x000D (Unofficial, for testing only)

You can run the utility with:

```console
$ cargo make cli
```

## License

MIT license ([LICENSE](LICENSE))

## Contribution

Any contribution intentionally submitted for inclusion in the work by you shall be licensed as above, without any additional terms or conditions.
