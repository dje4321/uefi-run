use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uefi_run::*;

fn main() {
    // Parse Arguments from CLAP
    let args = Args::parse();

    // Install termination signal handler. This ensures that the destructor of
    // `temp_dir` which is constructed in the next step is really called and
    // the files are cleaned up properly.
    let terminating = Arc::new(AtomicBool::new(false));
    {
        let term = terminating.clone();
        ctrlc::set_handler(move || {
            println!("uefi-run terminating...");
            // Tell the main thread to stop waiting.
            term.store(true, Ordering::SeqCst);
        })
        .expect("Error setting termination handler");
    }

    // Get a temp file to store FatFS
    let temp_folder = tempfile::tempdir().unwrap();
    let temp_dir_path = temp_folder.path();
    let mut tempfile = PathBuf::from(temp_dir_path);
    tempfile.push("boot.img");

    // Setup a new UEFIImage
    let image = UEFIImage::new(&tempfile, args.size * 1024 * 1024);
    if let Some(data_folder) = &args.persistant_data {
        image.add_directory(data_folder, image.fs.root_dir()).expect("Failed to add persistant data folder to image");
    }

    if args.boot {
        image.add_bootloader(args.efi_exe).expect("Failed to inject EFI Exe");
    } else {
        image.add_startup_script(args.efi_exe).expect("Failed to inject startup script");
    }

    // Run QEMU
    let tempfile_path = tempfile.clone();
    let mut qemu_config = QemuConfig {
        qemu_path: args.qemu_path,
        bios_path: args.bios_path,
        drives: vec![QemuDriveConfig {
            file: tempfile_path.to_str().unwrap().to_string(),
            media: "disk".to_string(),
            format: "raw".to_string(),
        }],
        ..Default::default()
    };
    qemu_config
        .additional_args
        .extend(args.qemu_args.iter().cloned());

    // Run qemu
    let mut qemu_process = qemu_config.run().expect("Failed to start qemu");

    // Wait for qemu to exit or signal.
    let mut qemu_exit_code;
    loop {
        qemu_exit_code = qemu_process.wait(Duration::from_millis(500));
        if qemu_exit_code.is_some() || terminating.load(Ordering::SeqCst) {
            break;
        }
    }

    // The above loop may have been broken by a signal
    if qemu_exit_code.is_none() {
        // In this case we wait for qemu to exit for one second
        qemu_exit_code = qemu_process.wait(Duration::from_secs(1));
    }

    // Qemu may still be running
    if qemu_exit_code.is_none() {
        // In this case we need to kill it
        qemu_process
            .kill()
            .or_else(|e| match e.kind() {
                // Not running anymore
                std::io::ErrorKind::InvalidInput => Ok(()),
                _ => Err(e),
            })
            .expect("Unable to kill qemu process");
        qemu_exit_code = qemu_process.wait(Duration::from_secs(1));
    }

    // Sync files back to host after QEMU exits
    if let Some(data_folder) = &args.persistant_data {
        image.sync_directory(data_folder, image.fs.root_dir()).expect("Failed to sync persistant data folder to host");
    }

    std::fs::remove_file(tempfile).expect("Failed to cleanup FatFS Image");

    let exit_code = qemu_exit_code.expect("qemu should have exited by now but did not");
    std::process::exit(exit_code);
}