use std::ffi::OsStr;
use std::path::PathBuf;
use tempfile::TempDir;
use std::io::Write;
use std::fs;

#[cfg(target_os = "windows")]
const PNGQUANT_BIN: &[u8] = include_bytes!("../bin/pngquant.exe");
#[cfg(target_os = "windows")]
const OXIPNG_BIN: &[u8] = include_bytes!("../bin/oxipng.exe");

pub enum ToolPath {
    Path(PathBuf),
    #[allow(dead_code)]
    Command(String),
}

pub fn get_tool_ref(t: &ToolPath) -> &OsStr {
    match t {
        ToolPath::Path(p) => p.as_os_str(),
        ToolPath::Command(c) => OsStr::new(c),
    }
}

pub fn get_png_tools() -> Result<(Option<TempDir>, ToolPath, ToolPath), std::io::Error> {
    #[cfg(target_os = "windows")]
    {
        let dir = tempfile::tempdir()?;
        let pq_path = dir.path().join("pngquant.exe");
        let oxi_path = dir.path().join("oxipng.exe");
        let mut f1 = fs::File::create(&pq_path)?;
        f1.write_all(PNGQUANT_BIN)?;
        let mut f2 = fs::File::create(&oxi_path)?;
        f2.write_all(OXIPNG_BIN)?;
        Ok((Some(dir), ToolPath::Path(pq_path), ToolPath::Path(oxi_path)))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok((None, ToolPath::Command("pngquant".to_string()), ToolPath::Command("oxipng".to_string())))
    }
}