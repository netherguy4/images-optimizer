use clap::Parser;
use humansize::{format_size, DECIMAL};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;
use walkdir::WalkDir;
use image::GenericImageView;
use rgb::FromSlice; 

const PNGQUANT_BIN: &[u8] = include_bytes!("../bin/pngquant.exe");
const OXIPNG_BIN: &[u8] = include_bytes!("../bin/oxipng.exe");

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value = ".")]
    path: String,

    #[arg(long, default_value_t = 80)]
    jpg_q: u8,

    #[arg(long, default_value_t = 65)]
    png_min: u8,

    #[arg(long, default_value_t = 80)]
    png_max: u8,

    #[arg(long)]
    webp: bool,

    #[arg(long)]
    avif: bool,
}

fn unpack_png_tools() -> Result<(TempDir, PathBuf, PathBuf), std::io::Error> {
    let dir = tempfile::tempdir()?;
    let pq_path = dir.path().join("pngquant.exe");
    let oxi_path = dir.path().join("oxipng.exe");
    let mut f1 = fs::File::create(&pq_path)?;
    f1.write_all(PNGQUANT_BIN)?;
    let mut f2 = fs::File::create(&oxi_path)?;
    f2.write_all(OXIPNG_BIN)?;
    Ok((dir, pq_path, oxi_path))
}

fn process_jpg(path: &Path, quality: u8) -> u64 {
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
        let compressed_data = match comp.finish_compress() {
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

fn process_png(path: &Path, pq: &Path, oxi: &Path, min: u8, max: u8) -> u64 {
    let original_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new(pq);
    cmd.args([&format!("--quality={}-{}", min, max), "--speed=3", "--force", "--ext=.png", "--skip-if-larger"]).arg(path);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output();

    let mut cmd2 = Command::new(oxi);
    cmd2.args(["-o", "4", "--strip", "all", "-t", "1"]).arg(path);
    #[cfg(target_os = "windows")]
    cmd2.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd2.output();

    let new_size = fs::metadata(path).map(|m| m.len()).unwrap_or(original_size);
    if original_size > new_size { original_size - new_size } else { 0 }
}

fn generate_webp(img: &image::DynamicImage, path: &Path, quality: f32, original_size: u64) -> u64 {
    let webp_path = path.with_extension("webp");
    let (width, height) = img.dimensions();
    
    let memory = match img {
        image::DynamicImage::ImageRgba8(buf) => {
             webp::Encoder::from_rgba(buf.as_raw(), width, height).encode(quality)
        },
        image::DynamicImage::ImageRgb8(buf) => {
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

fn generate_avif(img: &image::DynamicImage, path: &Path, original_size: u64) -> u64 {
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
        Err(e) => eprintln!("Ошибка AVIF для {:?}: {}", path, e),
    }
    0
}

fn main() {
    let args = Args::parse();

    println!("Подготовка...");
    let (_tmp, pq, oxi) = match unpack_png_tools() {
        Ok(t) => t,
        Err(e) => { eprintln!("{}", e); return; }
    };

    println!("Сканирование: {}", args.path);
    let supported_exts = ["png", "jpg", "jpeg"];
    let files: Vec<PathBuf> = WalkDir::new(&args.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension()
                .map(|ext| supported_exts.contains(&ext.to_string_lossy().to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect();

    if files.is_empty() {
        println!("Файлы не найдены.");
        return;
    }

    println!("Найдено: {}. Старт...", files.len());
    let bar = ProgressBar::new(files.len() as u64);
    bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len}").unwrap().progress_chars("#>-"));

    let total_input_size = AtomicU64::new(0);
    let saved_orig = AtomicU64::new(0);
    let saved_webp = AtomicU64::new(0);
    let saved_avif = AtomicU64::new(0);

    files.par_iter().for_each(|path| {
        let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        let original_file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        
        total_input_size.fetch_add(original_file_size, Ordering::Relaxed);

        if args.webp || args.avif {
            if let Ok(img) = image::open(path) {
                if args.webp {
                    let s = generate_webp(&img, path, 75.0, original_file_size);
                    saved_webp.fetch_add(s, Ordering::Relaxed);
                }
                if args.avif {
                    let s = generate_avif(&img, path, original_file_size);
                    saved_avif.fetch_add(s, Ordering::Relaxed);
                }
            }
        }

        let s_orig = if ext == "png" {
            process_png(path, &pq, &oxi, args.png_min, args.png_max)
        } else {
            process_jpg(path, args.jpg_q)
        };
        saved_orig.fetch_add(s_orig, Ordering::Relaxed);

        bar.inc(1);
    });

    bar.finish_with_message("Готово");

    let total_in = total_input_size.load(Ordering::Relaxed);
    let s_orig = saved_orig.load(Ordering::Relaxed);
    let s_webp = saved_webp.load(Ordering::Relaxed);
    let s_avif = saved_avif.load(Ordering::Relaxed);

    println!("\n📊 Итоговые результаты:");
    
    let calc_perc = |saved: u64| -> f64 {
        if total_in > 0 { (saved as f64 / total_in as f64) * 100.0 } else { 0.0 }
    };

    println!("   Исходный общий вес:  {}", format_size(total_in, DECIMAL));
    println!("   ------------------------------------------------");
    
    println!("   После сжатия (JPG/PNG): {} (🔻{:.1}%)", 
        format_size(total_in - s_orig, DECIMAL), 
        calc_perc(s_orig)
    );
    
    if args.webp {
        println!("   Версия WebP (Итого):    {} (🔻{:.1}%)", 
            format_size(total_in - s_webp, DECIMAL), 
            calc_perc(s_webp)
        );
    }
    
    if args.avif {
        println!("   Версия AVIF (Итого):    {} (🔻{:.1}%)", 
            format_size(total_in - s_avif, DECIMAL), 
            calc_perc(s_avif)
        );
    }
    
    println!("\nНажмите Enter для выхода...");
    let _ = std::io::stdin().read_line(&mut String::new());
}