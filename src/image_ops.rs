use std::fs;
use std::path::Path;
use std::process::Command;
use std::io::Write;
use image::{GenericImageView, DynamicImage};
use rgb::FromSlice; 
use crate::tools::{ToolPath, get_tool_ref};

pub fn process_jpg(path: &Path, quality: u8) -> u64 {
    let original_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let img = match image::open(path) {
        Ok(i) => i.to_rgb8(),
        Err(_) => return 0,
    };
    let width = img.width() as usize;
    let height = img.height() as usize;
    let pixels = img.as_raw();

    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_quality(quality as f32);
    comp.set_progressive_mode();
    comp.set_optimize_scans(true);
    let mut comp = comp.start_compress(Vec::new()).unwrap();
    
    if comp.write_scanlines(pixels).is_ok() {
        let compressed_data = match comp.finish() {
            Ok(d) => d,
            Err(_) => return 0,
        };
        let new_len = compressed_data.len() as u64;
        if new_len > 0 && new_len < original_size {
             if let Ok(mut f) = fs::File::create(path) {
                 if f.write_all(&compressed_data).is_ok() {
                     return original_size - new_len;
                 }
             }
        }
    }
    0
}

pub fn process_png(path: &Path, pq: &ToolPath, oxi: &ToolPath, min: u8, max: u8) -> u64 {
    let original_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    #[cfg(target_os = "windows")]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new(get_tool_ref(pq));
    cmd.args([&format!("--quality={}-{}", min, max), "--speed=3", "--force", "--ext=.png", "--skip-if-larger"]).arg(path);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output();

    let mut cmd2 = Command::new(get_tool_ref(oxi));
    cmd2.args(["-o", "4", "--strip", "all", "-t", "1"]).arg(path);
    #[cfg(target_os = "windows")]
    cmd2.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd2.output();

    let new_size = fs::metadata(path).map(|m| m.len()).unwrap_or(original_size);
    if original_size > new_size { original_size - new_size } else { 0 }
}

pub fn generate_webp(img: &DynamicImage, path: &Path, quality: f32, original_size: u64) -> u64 {
    let webp_path = path.with_extension("webp");
    let (width, height) = img.dimensions();
    
    let memory = match img {
        DynamicImage::ImageRgba8(buf) => {
             webp::Encoder::from_rgba(buf.as_raw(), width, height).encode(quality)
        },
        DynamicImage::ImageRgb8(buf) => {
             webp::Encoder::from_rgb(buf.as_raw(), width, height).encode(quality)
        },
        _ => {
            let buf = img.to_rgba8();
            webp::Encoder::from_rgba(buf.as_raw(), width, height).encode(quality)
        }
    };

    if fs::write(&webp_path, &*memory).is_ok() {
        let webp_size = memory.len() as u64;
        if original_size > webp_size {
            return original_size - webp_size;
        }
    }
    0
}

pub fn generate_avif(img: &DynamicImage, path: &Path, original_size: u64) -> u64 {
    let avif_path = path.with_extension("avif");
    let rgba = img.to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    
    let src_img = imgref::Img::new(
        rgba.as_raw().as_slice().as_rgba(),
        width,
        height,
    );

    let enc = ravif::Encoder::new()
        .with_quality(65.0) 
        .with_speed(4)
        .with_alpha_quality(70.0)
        .encode_rgba(src_img);

    match enc {
        Ok(encoded_image) => {
            let data = encoded_image.avif_file;
            if fs::write(&avif_path, &data).is_ok() {
                let avif_size = data.len() as u64;
                if original_size > avif_size {
                    return original_size - avif_size;
                }
            }
        },
        Err(e) => eprintln!("AVIF Error for {:?}: {}", path, e),
    }
    0
}