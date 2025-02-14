#!/bin/bash
# a quick and dirty script to init stuff

# Check the Linux distro
distro=$(cat /etc/*release | grep -i "^ID=" | cut -d= -f2 | tr -d '"')

echo "Detected Linux Distro: $distro"

# Install required packages based on the distribution
if [[ "$distro" == "ubuntu" || "$distro" == "debian" ]]; then
    sudo apt update
    sudo apt install -y qemu rustc rustup ovmf
elif [[ "$distro" == "fedora" || "$distro" == "centos" || "$distro" == "rhel" ]]; then
    sudo dnf install -y qemu rust rustup ovmf
elif [[ "$distro" == "arch" || "$distro" == "manjaro" ]]; then
    sudo pacman -S qemu rust rustup edk2-ovmf
else
    echo "Unsupported distribution: $distro"
    exit 1
fi

# Initialize rustup and install Rust (if not already installed)
if ! command -v rustup &> /dev/null; then
    echo "Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
fi

rustup target add x86_64-unknown-uefi

# emulate ESP 
mkdir -p "./esp/efi/boot"

if [[ "$distro" == "arch" || "$distro" == "manjaro" ]]; then
    ovmf_code="/usr/share/OVMF/x64/OVMF_CODE.4m.fd"
    ovmf_vars="/usr/share/OVMF/x64/OVMF_VARS.4m.fd"
else
    ovmf_code="/usr/share/OVMF/OVMF_CODE.fd"
    ovmf_vars="/usr/share/OVMF/OVMF_VARS.fd"
fi

if [ -f $ovmf_code ] && [ -f $ovmf_vars ]; then
    cp $ovmf_code "./OVMF_CODE.fd"
    cp $ovmf_vars "./OVMF_VARS.fd"
    echo "OVMF files copied to $./esp/efi/boot/"
else
    echo "OVMF files not found. Please check if they are installed."
    exit 1
fi

echo "......................."
echo "Setup complete!"
echo "Run 'make run' to build and run the UEFI application."
