# uefi-run 

**Directly run UEFI applications in qemu**

---

This helper application takes an EFI executable, builds a FAT filesystem around
it, adds a startup script and runs qemu to run the executable.

It does not require root permissions since it uses the [fatfs](https://crates.io/crates/fatfs)
crate to build the filesystem image directly without involving `mkfs`, `mount`,
etc.

This build has been patched to allow persistant storage of the UEFI Image contents that is generated during boot
