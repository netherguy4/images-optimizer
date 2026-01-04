use clap::{Parser, ValueHint};

#[derive(Parser, Debug)]
#[command(
    author, 
    version, 
    about = "High-performance parallel image optimizer.",
    long_about = "A multi-threaded CLI tool designed to compress JPG and PNG images recursively.\n\nIt utilizes mozjpeg, pngquant, and oxipng to reduce file sizes while preserving visual quality."
)]
pub struct Args {
    #[arg(required = false, value_delimiter = ',', num_args = 1.., value_hint = ValueHint::AnyPath, help = "List of files or directories to process.")]
    pub paths: Vec<String>,

    #[arg(long, default_value_t = 80, value_parser = clap::value_parser!(u8).range(1..=100), help_heading = "Quality Settings", help = "Target JPEG quality (0-100).")]
    pub jpg_q: u8,

    #[arg(long, default_value_t = 65, value_parser = clap::value_parser!(u8).range(1..=100), help_heading = "Quality Settings", help = "Minimum PNG quality allowed (0-100).")]
    pub png_min: u8,

    #[arg(long, default_value_t = 80, value_parser = clap::value_parser!(u8).range(1..=100), help_heading = "Quality Settings", help = "Maximum PNG quality allowed (0-100).")]
    pub png_max: u8,

    #[arg(long, help_heading = "Format Generation", help = "Generate WebP versions alongside originals.")]
    pub webp: bool,

    #[arg(long, help_heading = "Format Generation", help = "Generate AVIF versions alongside originals.")]
    pub avif: bool,

    #[arg(long, help = "Overwrite original files in place.")]
    pub replace: bool,

    #[arg(short = 'S', long, help = "Suppress all standard output.")]
    pub silent: bool,
}