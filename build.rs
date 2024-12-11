use std::process::Command;

fn main() {
    // Specify the path to the assembly file and output binary
    let out_dir = "target/x86_64-funderberker/debug";
    let boot_bin = format!("{}/boot.bin", out_dir);

    // Run the assembler (NASM in this example)
    let status = Command::new("nasm")
        .args(&["-f", "bin", "-o", &boot_bin, "src/boot.S"])
        .status()
        .expect("Failed to execute NASM");

    if !status.success() {
        panic!("NASM failed to assemble boot.S");
    }

    let status = Command::new("dd")
        .args(&["if=/dev/zero", "of=disk.img", "bs=512", "count=2880"])
        .status()
        .expect("Failed to create FS file");

    if !status.success() {
        panic!("You already know why the command failed dumbass");
    }

    let status = Command::new("mkfs.fat")
        .args(&["-F", "12", "-n", "FBHV", "disk.img"])
        .status()
        .expect("Failed to format FS file");

    if !status.success() {
        panic!("You already know why the command failed dumbass");
    }

    let status = Command::new("dd")
        .args(&["if=./target/x86_64-funderberker/debug/boot.bin", "of=fat12.img", "conv=notrunc"])
        .status()
        .expect("Failed to copy bootloader boot sector to FAT12 FS");

    if !status.success() {
        panic!("You already know why the command failed dumbass");
    }

    println!("cargo:rerun-if-changed=boot.S");
}

