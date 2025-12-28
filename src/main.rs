use clap::Parser;
use humansize::{format_size, DECIMAL};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
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

    #[arg(long)]
    replace: bool,

    /// Silent mode: shows only progress bar, no stats, no wait for enter
    #[arg(short = 'S', long)]
    silent: bool,
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
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
        Err(e) => eprintln!("AVIF Error for {:?}: {}", path, e),
    }
    0
}

fn main() {
    let args = Args::parse();
    let total_start_time = Instant::now();

    // Show warnings only if NOT silent
    if args.avif && !args.silent {
        println!("\x1b[93m⚠️  WARNING: AVIF encoding is active.\x1b[0m");
        println!("\x1b[93m   This process is extremely CPU intensive and may take significantly longer.\x1b[0m");
        println!("\x1b[93m   Ensure your system has adequate cooling and power.\x1b[0m");
        println!("------------------------------------------------");
    }

    if !args.silent { println!("Preparing tools..."); }
    let (_tmp, pq, oxi) = match unpack_png_tools() {
        Ok(t) => t,
        Err(e) => { eprintln!("{}", e); return; }
    };

    let input_path = PathBuf::from(&args.path);
    let target_dir: PathBuf;
    let copy_duration;

    if args.replace {
        target_dir = input_path.clone();
        copy_duration = std::time::Duration::new(0, 0);
        if !args.silent { println!("Mode: \x1b[31mREPLACE\x1b[0m (Overwriting files in {:?})", target_dir); }
    } else {
        let root_name = input_path.file_name().unwrap_or_default().to_string_lossy();
        let new_name = format!("{}__optimized", root_name);
        target_dir = input_path.parent().unwrap_or(Path::new(".")).join(new_name);
        
        if target_dir.exists() {
            if !args.silent { println!("Cleaning up existing output directory: {:?}", target_dir); }
            if let Err(e) = fs::remove_dir_all(&target_dir) {
                eprintln!("Error removing directory: {}", e);
                return;
            }
        }

        if !args.silent { println!("Mode: \x1b[32mSAFE\x1b[0m (Copying to {:?})", target_dir); }
        let copy_start = Instant::now();
        if let Err(e) = copy_dir_recursive(&input_path, &target_dir) {
            eprintln!("Error copying directory: {}", e);
            return;
        }
        copy_duration = copy_start.elapsed();
        if !args.silent { println!("Copy complete in {:.2?}", copy_duration); }
    }

    if !args.silent { println!("Scanning directory: {:?}", target_dir); }
    let scan_start = Instant::now();
    let supported_exts = ["png", "jpg", "jpeg"];
    let files: Vec<PathBuf> = WalkDir::new(&target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension()
                .map(|ext| supported_exts.contains(&ext.to_string_lossy().to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect();
    let scan_duration = scan_start.elapsed();

    if files.is_empty() {
        if !args.silent { println!("No supported files found."); }
        return;
    }

    if !args.silent { println!("Found: {} files. Processing...", files.len()); }
    
    // Progress bar remains even in silent mode
    let bar = ProgressBar::new(files.len() as u64);
    bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len}").unwrap().progress_chars("#>-"));

    let total_input_size = AtomicU64::new(0);
    
    let saved_orig = AtomicU64::new(0);
    let saved_webp = AtomicU64::new(0);
    let saved_avif = AtomicU64::new(0);

    let time_jpg = AtomicU64::new(0);
    let time_png = AtomicU64::new(0);
    let time_webp = AtomicU64::new(0);
    let time_avif = AtomicU64::new(0);

    let process_start_time = Instant::now();

    files.par_iter().for_each(|path| {
        let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        let original_file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        
        total_input_size.fetch_add(original_file_size, Ordering::Relaxed);

        if args.webp || args.avif {
            if let Ok(img) = image::open(path) {
                if args.webp {
                    let t = Instant::now();
                    let s = generate_webp(&img, path, 75.0, original_file_size);
                    time_webp.fetch_add(t.elapsed().as_millis() as u64, Ordering::Relaxed);
                    saved_webp.fetch_add(s, Ordering::Relaxed);
                }
                if args.avif {
                    let t = Instant::now();
                    let s = generate_avif(&img, path, original_file_size);
                    time_avif.fetch_add(t.elapsed().as_millis() as u64, Ordering::Relaxed);
                    saved_avif.fetch_add(s, Ordering::Relaxed);
                }
            }
        }

        let t_orig = Instant::now();
        let s_orig = if ext == "png" {
            let res = process_png(path, &pq, &oxi, args.png_min, args.png_max);
            time_png.fetch_add(t_orig.elapsed().as_millis() as u64, Ordering::Relaxed);
            res
        } else {
            let res = process_jpg(path, args.jpg_q);
            time_jpg.fetch_add(t_orig.elapsed().as_millis() as u64, Ordering::Relaxed);
            res
        };
        saved_orig.fetch_add(s_orig, Ordering::Relaxed);

        bar.inc(1);
    });

    if args.silent {
        bar.finish_and_clear();
    } else {
        bar.finish_with_message("Done");
    }

    let process_duration = process_start_time.elapsed();
    let total_duration = total_start_time.elapsed();

    let total_in = total_input_size.load(Ordering::Relaxed);
    let s_orig = saved_orig.load(Ordering::Relaxed);
    let s_webp = saved_webp.load(Ordering::Relaxed);
    let s_avif = saved_avif.load(Ordering::Relaxed);

    let t_jpg = time_jpg.load(Ordering::Relaxed);
    let t_png = time_png.load(Ordering::Relaxed);
    let t_webp = time_webp.load(Ordering::Relaxed);
    let t_avif = time_avif.load(Ordering::Relaxed);

    // Show stats and wait for Enter ONLY if NOT silent
    if !args.silent {
        println!("\n📊 Final Results:");
        
        let calc_perc = |saved: u64| -> f64 {
            if total_in > 0 { (saved as f64 / total_in as f64) * 100.0 } else { 0.0 }
        };

        println!("   Total input size:    {}", format_size(total_in, DECIMAL));
        println!("   Total wall time:     {:.2?}", total_duration);
        if !args.replace {
            println!("     L Copying time:    {:.2?}", copy_duration);
        }
        println!("     L Scan time:       {:.2?}", scan_duration);
        println!("     L Processing time: {:.2?}", process_duration);
        println!("   ------------------------------------------------");
        
        println!("   Optimization (JPG/PNG): {} (🔻{:.1}%)", 
            format_size(total_in - s_orig, DECIMAL), 
            calc_perc(s_orig)
        );
        if t_jpg > 0 { println!("     L JPG Cumulative Time: {:.2}s", t_jpg as f64 / 1000.0); }
        if t_png > 0 { println!("     L PNG Cumulative Time: {:.2}s", t_png as f64 / 1000.0); }
        
        if args.webp {
            println!("   WebP Generation:        {} (🔻{:.1}%)", 
                format_size(total_in - s_webp, DECIMAL), 
                calc_perc(s_webp)
            );
            println!("     L Time taken:          {:.2}s", t_webp as f64 / 1000.0);
        }
        
        if args.avif {
            println!("   AVIF Generation:        {} (🔻{:.1}%)", 
                format_size(total_in - s_avif, DECIMAL), 
                calc_perc(s_avif)
            );
            println!("     L Time taken:          {:.2}s", t_avif as f64 / 1000.0);
        }
        
        println!("\n   * Note: 'Cumulative Time' represents the sum of work across all CPU cores.");
        println!("     It differs from 'Wall time' due to parallel processing.");

        println!("\nPress Enter to exit...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}