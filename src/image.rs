use core::panic;
use std::{fs::File, path::{Path, PathBuf}, io::Write, io::Read, io::{Seek, SeekFrom}, println};
use anyhow::{Result};

use fatfs::Dir;

/// Default startup script that just runs `BOOTX64.efi`
pub const DEFAULT_STARTUP_NSH: &[u8] = include_bytes!("startup.nsh");

/// Handle to a FAT filesystem used as an EFI partition
pub struct UEFIImage {
    pub fs: fatfs::FileSystem<File>,
}

impl UEFIImage {
    pub fn new<P: AsRef<Path> >(file: P, size: u64) -> UEFIImage {

        let mut file = std::fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .open(&file).expect("Failed to open temp file");

        file.set_len(size).expect("Failed to set length");

        // Ensure we can reserve enough disk space for file image
        let zero_buf = vec![0; 1024 * 1024];
        for _ in 0..(size/1024/1024) {
            let r = file.write(&zero_buf);
            if r.is_err() {
                panic!("Not enough disk space to allocate FAT FS");
            }
        }

        file.seek(SeekFrom::Start(0)).expect("Failed to seek to start of file");

        let label: [u8; 11] = ['B' as u8, 'O' as u8, 'O' as u8, 'T' as u8, 0, 0, 0, 0, 0, 0, 0];

        fatfs::format_volume(
            &file,
            fatfs::FormatVolumeOptions
                ::new()
                .fat_type(fatfs::FatType::Fat32)
                .volume_label(label) // BOOT
        ).expect("Failed to format FAT Image");

        let fs = fatfs::FileSystem::new(
            file, 
            fatfs::FsOptions::new()
        ).expect("Failed to generate FAT Filesystem");

        UEFIImage { 
            fs,
        }
    }
    pub fn add_directory<P: AsRef<Path>>(&self, pers_data: P, fs_dir: Dir<File>) -> Result<()>{
        let dir = std::fs::read_dir(pers_data)?;

        // Go over every file in the directory
        for file in dir {
            let file = file?;

            let file_path = file.path().clone();
            let file_name = file_path.file_name().expect("Failed to get file name");
            let file_name = file_name.to_str().expect("Incompatible characters in file name");

            // Check if the file is a directory
            if file.file_type()?.is_dir() {
                // Mirror folder onto the FAT FS
                fs_dir.create_dir(file_name)?;

                // Call AddDirectory recursively to set inner contents
                self.add_directory(
                    file.path(), 
                    fs_dir.open_dir(file_name)?
                )?;
            }

            if file.file_type()?.is_symlink() {
                //Follow symlink and get the real file it points too. 
                let real_file_path = std::fs::canonicalize(file.path());
                if real_file_path.is_err() {
                    panic!("Symlink Points to invalid file\r\nSymlink: {:?}", file_path)
                }

                // Mirror file onto the FAT FS
                let mut fat_file = fs_dir.create_file(file_name).expect("Failed to create file on FatFS");
                
                let file_contents = std::fs::read(&real_file_path.unwrap())?;
                fat_file.write_all(&file_contents).expect("Failed to write file to FatFS. Disk size too small");
            }

            if file.file_type()?.is_file() {
                // Mirror file onto the FAT FS
                let mut fat_file = fs_dir.create_file(file_name).expect("Failed to create file on FatFS");

                let file_contents = std::fs::read(&file_path)?;
                fat_file.write_all(&file_contents).expect("Filed to write data to FAT FS. Disk size too small");
            }
        }
    
        return Ok(());
    }
    pub fn sync_directory<P: AsRef<Path> >(&self, pers_data: P, fs_dir: Dir<File>) -> Result<()> {
        'file: for file in fs_dir.iter() {
            let file = file.expect("Failed to get FAT file");
            let filename = file.file_name();

            if filename == "." || filename == ".." || filename == "NvVars" || filename == "BootX64.efi" || filename == "startup.nsh"    {
                continue 'file;
            }

            let mut file_path = PathBuf::new();
            file_path.push(&pers_data);
            file_path.push(&filename);

            if file.is_dir() {
                // Ensure directory exsists on host
                let r_os_folder_md = std::fs::metadata(&file_path);
                match r_os_folder_md {
                    Ok(os_folder_md) => {
                        // File exsists
                        if !os_folder_md.is_dir() {
                            panic!("File conflict found on host folder. Expected directory");
                        }
                    }
                    Err(_) => {
                        // File does not exsist
                        std::fs::create_dir(&file_path)?;
                    }
                }

                self.sync_directory(&file_path, fs_dir.open_dir(filename.as_str())?)?;                
            }

            if file.is_file() {
                // Ensure file exsists on host
                let r_os_file_md = std::fs::metadata(&file_path);
                match r_os_file_md {
                    Ok(os_file_md) => {

                        if os_file_md.is_symlink() {
                            // Do nothing on symlinks to prevent arbritary values from being written to any file on the FS
                            println!("[SYNC]: {filename} is a Symlink. Skipping");
                        }

                        // File exsists
                        if !os_file_md.is_file() {
                            panic!("File conflict found on host folder. Expected File");
                        }
                    }
                    Err(_) => {
                        // File does not exsist
                        ()
                    }
                }

                let os_file = std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .append(false)
                .create(true)
                .open(&file_path);

                if os_file.is_err() {
                    println!("[SYNC] Skipping... Failed to open Host file {:?}", &file_path);

                    continue 'file;
                }

                let mut data: Vec<u8> = Vec::new();
                file.to_file().read_to_end(&mut data).expect("Failed to read FAT file data");

                os_file.unwrap().write_all(data.as_slice()).expect("Failed to write to OS file");
            }
        }
        return Ok(());
    }
    pub fn add_bootloader<P: AsRef<Path>>(&self, efi_exe: P) -> Result<()> {
        let mut efi_exe = std::fs::OpenOptions::new()
        .write(false)
        .read(true)
        .create(false)
        .open(efi_exe).expect("Failed to open EFI Exe");

        // Check for /EFI
        let mut root_dir = self.fs.root_dir();
        let result = root_dir.open_dir("EFI");
        match result {
            Ok(dir) => {
                root_dir = dir;
            }
            Err(_) => {
                // Failed to open directory. Assume it doesnt exsist
                root_dir.create_dir("EFI")?;
                root_dir = root_dir.open_dir("EFI")?;
            }
        }

        // Check for /EFI/Boot/
        let result = root_dir.open_dir("Boot");
        match result {
            Ok(dir) => {
                root_dir = dir;
            }
            Err(_) => {
                // Failed to open directory. Assume it doesnt exsist
                root_dir.create_dir("Boot")?;
                root_dir = root_dir.open_dir("Boot")?;
            }
        }

        // Check for /EFI/Boot/BootX64.efi
        let result = root_dir.open_file("BootX64.efi");
        match result {
            Ok(mut file) => {
                let mut data: Vec<u8> = Vec::new();
                efi_exe.read_to_end(&mut data)?;

                file.write_all(&data)?;
            }
            Err(_) => {
                // Failed to open file. Assume it doesnt exsist
                let mut file = root_dir.create_file("BootX64.efi")?;
                
                let mut data: Vec<u8> = Vec::new();
                efi_exe.read_to_end(&mut data)?;

                file.write_all(&data)?;
            }
        }

        return Ok(());
    }
    pub fn add_startup_script<P: AsRef<Path>>(&self, efi_exe: P) -> Result<()> {
        let root_dir = self.fs.root_dir();
        let mut startup = root_dir.create_file("startup.nsh")?;
        startup.write_all(DEFAULT_STARTUP_NSH)?;

        let mut efi_exe = std::fs::OpenOptions::new()
        .write(false)
        .read(true)
        .create(false)
        .open(efi_exe).expect("Failed to open EFI Exe");

        let result = root_dir.open_file("BootX64.efi");
        match result {
            Ok(mut file) => {
                let mut data: Vec<u8> = Vec::new();
                efi_exe.read_to_end(&mut data)?;

                file.write_all(&data)?;
            }
            Err(_) => {
                // Failed to open file. Assume it doesnt exsist
                let mut file = root_dir.create_file("BootX64.efi")?;
                
                let mut data: Vec<u8> = Vec::new();
                efi_exe.read_to_end(&mut data)?;

                file.write_all(&data)?;
            }
        }

        return Ok(());
    }
}