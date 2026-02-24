# lockrs

A minimal X11 lockscreen make with xcb and rust.

## Installation

Clone the repo and then run:

```
cargo install --path .
```

ensure that `~/.cargo/bin` is on your PATH to be able to launch the application.

## Usage

You can run it from the command line or a launcher with `lockrs` or you could setup an auto lockscreen functionality like this `xautolock -time 5 -locker lockrs`. Note: xautolock is external software that you would need to download.
