use clap::Parser;
/// Command line arguments for uefi-run
#[derive(Parser, Debug, Default, PartialEq)]
#[clap(
    version,
    author,
    about,
    trailing_var_arg = true,
    dont_delimit_trailing_values = true
)]
pub struct Args {
    /// Bios image
    #[clap(long, short = 'b', default_value = "OVMF.fd")]
    pub bios_path: String,
    /// Path to qemu executable
    #[clap(long, short = 'q', default_value = "qemu-system-x86_64")]
    pub qemu_path: String,
    /// Size of the image in MB
    #[clap(long, short = 's', default_value_t = 256)]
    pub size: u64,
    /// Folder to include inside the efi image
    /// 
    /// Any changes made inside the image will be reflected back onto the host OS
    /// allowing for persistant storage of files
    ///  
    /// Useful for debug logging and general UEFI file manipulation
    #[clap(long, short = 'p')]
    pub persistant_data: Option<String>,
    /// EFI Executable
    pub efi_exe: String,
    /// Additional arguments for qemu   
    pub qemu_args: Vec<String>,
    /// Load the application as a bootloader instead of in an EFI shell
    ///
    /// This effectively skips the 5 second startup delay.
    #[clap(long, short = 'd')]
    pub boot: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_add_file_args() {
        let mut args = Args::default();
        args.add_file = vec![
            "/full/path/to/outer:/full/path/to/inner".to_string(),
            "/full/path/to/outer:inner".to_string(),
            "outer:inner".to_string(),
            "/full/path/to/outer".to_string(),
            "outer".to_string(),
        ];
        #[rustfmt::skip]
        let expected = vec![
            (PathBuf::from("/full/path/to/outer"), PathBuf::from("/full/path/to/inner")),
            (PathBuf::from("/full/path/to/outer"), PathBuf::from("inner")),
            (PathBuf::from("outer"), PathBuf::from("inner")),
            (PathBuf::from("/full/path/to/outer"), PathBuf::from("outer")),
            (PathBuf::from("outer"), PathBuf::from("outer")),
        ];
        let actual = args
            .parse_add_file_args()
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}
