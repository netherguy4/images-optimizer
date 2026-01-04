mod cli;
mod tools;
mod fs_utils;
mod image_ops;

use clap::{Parser, CommandFactory};
use console::{style, Term};
use humansize::{format_size, DECIMAL};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant}; 
use walkdir::WalkDir;

use cli::Args;
use tools::get_png_tools;
use fs_utils::copy_dir_recursive;
use image_ops::{process_jpg, process_png, generate_webp, generate_avif};

fn main() {
    let args = Args::parse();

    if args.paths.is_empty() {
        let mut cmd = Args::command();
        cmd.print_help().unwrap();
        return;
    }

    let total_start_time = Instant::now();

    if args.avif && !args.silent {
        println!("{}", style("[!] WARNING: AVIF encoding is active.").red().bold());
        println!("{}", style("    This process is extremely CPU intensive and may take significantly longer.").yellow());
        println!("{}", style("    Ensure your system has adequate cooling and power.").yellow());
        println!("{}", style("------------------------------------------------").dim());
    }

    if !args.silent { println!("{}", style("Preparing tools...").cyan()); }
    let (_tmp_dir, pq, oxi) = match get_png_tools() {
        Ok(t) => t,
        Err(e) => { eprintln!("{}", style(e).red()); return; }
    };

    let supported_exts = ["png", "jpg", "jpeg"];
    let mut files_to_process: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut copy_duration = Duration::new(0, 0);
    let scan_start = Instant::now();

    let is_single_dir_mode = args.paths.len() == 1 && Path::new(&args.paths[0]).is_dir();

    if is_single_dir_mode {
        let input_path = PathBuf::from(&args.paths[0]);
        let target_dir: PathBuf;

        if args.replace {
            target_dir = input_path.clone();
            if !args.silent { 
                println!("Mode: {} (Overwriting files in {})", style("REPLACE").red().bold(), style(target_dir.to_string_lossy()).cyan()); 
            }
        } else {
            let root_name = input_path.file_name().unwrap_or_default().to_string_lossy();
            let new_name = format!("{}__optimized", root_name);
            target_dir = input_path.parent().unwrap_or(Path::new(".")).join(new_name);
            
            if !args.silent { 
                println!("Mode: {} (Copying to {})", style("SAFE").green().bold(), style(target_dir.to_string_lossy()).cyan()); 
            }
            let copy_start = Instant::now();
            if let Err(e) = copy_dir_recursive(&input_path, &target_dir) {
                eprintln!("{} {}", style("Error copying directory:").red(), e);
                return;
            }
            copy_duration = copy_start.elapsed();
            if !args.silent { println!("Copy complete in {}", style(format!("{:.2?}", copy_duration)).yellow()); }
        }

        if !args.silent { 
            println!("Scanning directory: {}", style(target_dir.to_string_lossy()).cyan()); 
        }
        let scanned: Vec<(PathBuf, PathBuf)> = WalkDir::new(&target_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension()
                    .map(|ext| supported_exts.contains(&ext.to_string_lossy().to_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .map(|e| {
                let p = e.into_path();
                (p.clone(), p)
            })
            .collect();
        files_to_process.extend(scanned);

    } else {
        if !args.silent { println!("Mode: {}", style("Specific File/Folder List Processing").magenta()); }
        let copy_start = Instant::now();
        
        for p_str in &args.paths {
            let path = Path::new(p_str);
            if !path.exists() {
                if !args.silent { eprintln!("{}", style(format!("Skipping not found: {:?}", path)).yellow()); }
                continue;
            }

            if path.is_dir() {
                let target_dir_root = if args.replace {
                    path.to_path_buf()
                } else {
                    let root_name = path.file_name().unwrap_or_default().to_string_lossy();
                    let new_name = format!("{}__optimized", root_name);
                    path.parent().unwrap_or(Path::new(".")).join(new_name)
                };

                if !args.silent {
                    println!("  > Processing Directory: {} -> {}", 
                        style(path.file_name().unwrap_or_default().to_string_lossy()).cyan(),
                        style(target_dir_root.file_name().unwrap_or_default().to_string_lossy()).yellow()
                    );
                }

                if !args.replace {
                    if let Err(e) = copy_dir_recursive(path, &target_dir_root) {
                        eprintln!("{} {:?}: {}", style("Error copying directory").red(), path, e);
                        continue;
                    }
                }

                let scanned: Vec<(PathBuf, PathBuf)> = WalkDir::new(&target_dir_root)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path().extension()
                            .map(|ext| supported_exts.contains(&ext.to_string_lossy().to_lowercase().as_str()))
                            .unwrap_or(false)
                    })
                    .map(|e| {
                        let p = e.into_path();
                        (p.clone(), p)
                    })
                    .collect();
                files_to_process.extend(scanned);
                continue;
            }
            
            let ext = path.extension().unwrap_or(OsStr::new("")).to_string_lossy().to_lowercase();
            if !supported_exts.contains(&ext.as_str()) {
                if !args.silent { eprintln!("{}", style(format!("Skipping unsupported type: {:?}", path)).yellow()); }
                continue;
            }

            let target_path = if args.replace {
                path.to_path_buf()
            } else {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let new_name = format!("{}__optimized.{}", stem, ext);
                path.parent().unwrap_or(Path::new(".")).join(new_name)
            };

            let naming_base = if args.replace {
                target_path.clone()
            } else {
                path.to_path_buf()
            };

            if !args.replace {
                if let Err(e) = fs::copy(path, &target_path) {
                    eprintln!("{} {:?}: {}", style("Error creating safe copy for").red(), path, e);
                    continue;
                }
            }
            files_to_process.push((target_path, naming_base));
        }
        
        if !args.replace {
            copy_duration = copy_start.elapsed();
        }
    }

    let scan_duration = scan_start.elapsed();

    if files_to_process.is_empty() {
        if !args.silent { println!("{}", style("No supported files found to process.").red()); }
        return;
    }

    if !args.silent { println!("Found: {} files. Processing...", style(files_to_process.len()).bold().yellow()); }
    
    let bar = ProgressBar::new(files_to_process.len() as u64);
    
    bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise:.bold}] [{bar:40.cyan/white}] {pos}/{len} ({eta}) {msg}")
        .unwrap()
        .tick_chars("|/-\\ ")
        .progress_chars("#>-"));

    bar.enable_steady_tick(Duration::from_millis(100));

    let total_input_size = AtomicU64::new(0);
    
    let saved_orig = AtomicU64::new(0);
    let saved_webp = AtomicU64::new(0);
    let saved_avif = AtomicU64::new(0);

    let time_jpg = AtomicU64::new(0);
    let time_png = AtomicU64::new(0);
    let time_webp = AtomicU64::new(0);
    let time_avif = AtomicU64::new(0);

    let process_start_time = Instant::now();

    files_to_process.par_iter().for_each(|(path, naming_path)| {
        let current_file_name = path.file_name().unwrap_or_default().to_string_lossy();
        bar.set_message(format!("{}", style(current_file_name).dim()));

        let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        let original_file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        
        total_input_size.fetch_add(original_file_size, Ordering::Relaxed);

        if args.webp || args.avif {
            if let Ok(img) = image::open(path) {
                if args.webp {
                    let t = Instant::now();
                    let s = generate_webp(&img, naming_path, 75.0, original_file_size);
                    time_webp.fetch_add(t.elapsed().as_millis() as u64, Ordering::Relaxed);
                    saved_webp.fetch_add(s, Ordering::Relaxed);
                }
                if args.avif {
                    let t = Instant::now();
                    let s = generate_avif(&img, naming_path, original_file_size);
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
        bar.finish_with_message(format!("{}", style("Done").green().bold()));
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

    if !args.silent {
        println!("\n{}", style("=== Final Results ===").bold().magenta());
        
        let calc_perc = |saved: u64| -> f64 {
            if total_in > 0 { (saved as f64 / total_in as f64) * 100.0 } else { 0.0 }
        };

        println!("    Total input size:    {}", style(format_size(total_in, DECIMAL)).cyan().bold());
        println!("    Total wall time:     {}", style(format!("{:.2?}", total_duration)).yellow());
        if !args.replace {
            println!("      L Copy/Prep time:   {}", style(format!("{:.2?}", copy_duration)).dim());
        }
        if is_single_dir_mode {
            println!("      L Scan time:        {}", style(format!("{:.2?}", scan_duration)).dim());
        }
        println!("      L Processing time: {}", style(format!("{:.2?}", process_duration)).yellow());
        println!("{}", style("    ------------------------------------------------").dim());
        
        println!("    Optimization (JPG/PNG): {} ({})", 
            style(format_size(total_in - s_orig, DECIMAL)).green().bold(), 
            style(format!("-{:.1}%", calc_perc(s_orig))).green()
        );
        if t_jpg > 0 { println!("      L JPG Cumulative Time: {:.2}s", t_jpg as f64 / 1000.0); }
        if t_png > 0 { println!("      L PNG Cumulative Time: {:.2}s", t_png as f64 / 1000.0); }
        
        if args.webp {
            println!("    WebP Generation:        {} ({})", 
                style(format_size(total_in - s_webp, DECIMAL)).green().bold(), 
                style(format!("-{:.1}%", calc_perc(s_webp))).green()
            );
            println!("      L Cumulative Time:      {:.2}s", t_webp as f64 / 1000.0);
        }
        
        if args.avif {
            println!("    AVIF Generation:        {} ({})", 
                style(format_size(total_in - s_avif, DECIMAL)).green().bold(), 
                style(format!("-{:.1}%", calc_perc(s_avif))).green()
            );
            println!("      L Cumulative Time:      {:.2}s", t_avif as f64 / 1000.0);
        }
        
        println!("\n{}", style("    * Note: 'Cumulative Time' represents the sum of work across all CPU cores.").dim().italic());
        println!("{}", style("      It differs from 'Wall time' due to parallel processing.").dim().italic());

        println!("\nPress any key to exit...");
        let term = Term::stdout();
        let _ = term.read_char();
    }
}