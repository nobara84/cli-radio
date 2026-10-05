use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
pub struct Log {
    file: Option<File>,
    path: std::path::PathBuf,
    size: u64,
}
impl Log {
    pub fn open(dir: &Path) -> Self {
        let path = dir.join("cli-radio.log");
        if fs::metadata(&path).is_ok_and(|m| m.len() > 2_000_000) {
            let _ = fs::rename(&path, dir.join("cli-radio.previous.log"));
        }
        Self {
            size: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            path: path.clone(),
            file: OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(path)
                .ok(),
        }
    }
    // Only static messages and numeric generations are accepted. Never raw mpv errors/URLs.
    pub fn event(&mut self, message: &'static str, generation: u64) {
        if self.size >= 2_000_000 {
            self.file.take();
            let _ = fs::rename(
                &self.path,
                self.path.with_file_name("cli-radio.previous.log"),
            );
            self.file = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(&self.path)
                .ok();
            self.size = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        }
        if let Some(file) = &mut self.file {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let line = format!("{time} generation={generation} {message}\n");
            if file.write_all(line.as_bytes()).is_ok() {
                self.size += line.len() as u64;
            }
        }
    }
}
