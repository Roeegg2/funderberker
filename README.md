[![GitHub stars](https://img.shields.io/github/stars/roeegg2/funderberker.svg)](https://github.com/funderberker-org/funderberker/stargazers)
[![License](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
![CI Status](https://img.shields.io/github/actions/workflow/status/funderberker-org/funderberker/ci.yaml?logo=github)


# Funderberker

Funderberker is a WIP type 1 hypervisor written in Rust, with a focus on customizability and performance. 
It uses as few dependencies as possible, implementing close to everything from scratch aiming to reduce code bloat, and improves performance and stability.

Currently only `x86_64` (both `Intel` and `AMD` CPUs) is supported, but support for `aarch64` (and possibly `RISC-V` when it's HA virtualization is more mature) is planned.

See [this list](kernel/Cargo.toml) of all available features.

## Building

Just (pun intended :) run:

```
just run
```

For more info, run 
```
just help
```

## Structure

Naturally, because of the nature of a type 1 hypervisor, the code structure is similar to that of a microkernel:
- `kernel`: A very minimal, basic kernel that provides the bare minimum to run the hypervisor.
- `hypervisor`: The hypervisor itself, which provides the basic functionality to run VMs.
- `drivers`: Drivers for various devices
- `utils`: Various utilities and helpers
- `macros`: Custom proc macros (placed in a crate of it's own because of a Rust internal limitation)
- `logger`: A simple loggin crate to log messages during runtime
- `pmm`: A buddy physical memory manager
- `slab`: A slab allocator
- `scheduler`: A simple scheduler to manage VMs


## Contributing

Unfortunately, as of now the project is not open for contributions, as I'm working on it as a SoM project, but if you want to help, feel free to open an issue or a discussion for any feature/bug/CR/anything else. I'd really appreciate it!
Once SoM is over I'll merge in any PRs.
