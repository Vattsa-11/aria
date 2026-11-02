use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
pub fn pick_file(filter_name: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(filter_name, extensions)
        .pick_file()
}

#[cfg(not(target_os = "android"))]
pub fn pick_files(filter_name: &str, extensions: &[&str]) -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .add_filter(filter_name, extensions)
        .pick_files()
}

#[cfg(not(target_os = "android"))]
pub fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

#[cfg(target_os = "android")]
pub fn pick_file(_filter_name: &str, _extensions: &[&str]) -> Option<PathBuf> {
    None
}

#[cfg(target_os = "android")]
pub fn pick_files(_filter_name: &str, _extensions: &[&str]) -> Option<Vec<PathBuf>> {
    None
}

#[cfg(target_os = "android")]
pub fn pick_folder() -> Option<PathBuf> {
    None
}
