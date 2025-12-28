# Rust Image Optimizer CLI

A high-performance, multi-threaded command-line tool designed to compress images (PNG, JPEG) and generate modern web formats (WebP, AVIF). Built with Rust for speed, safety, and efficiency.

## ⚠️ Disclaimer

**This tool is heavily optimized for Windows.** While it is written in Rust and theoretically cross-platform, stable operation on Linux and macOS is not guaranteed. If you encounter issues on non-Windows systems, please report them, but prioritize Windows for production use.

## 📦 Installation & Setup

### Add to PATH (Recommended)

For convenient access from any directory, add the tool to your system PATH:

**Windows**:
1. Locate the compiled `images-optimizer.exe` binary
2. Add its directory to your system PATH environment variable
3. Restart your terminal and use `images-optimizer` from anywhere

**Linux/macOS**:
1. Copy the binary to a directory in your PATH (e.g., `/usr/local/bin`)
   ```bash
   sudo cp images-optimizer /usr/local/bin/
   sudo chmod +x /usr/local/bin/images-optimizer
   ```
2. Use `images-optimizer` from any directory

### Quick Start

Verify installation:
```bash
images-optimizer --version
```

## 🚀 Features

- **Multi-threaded Processing**: Utilizes Rayon to maximize CPU core usage for parallel image processing.

- **Smart Optimization**:
  - PNG: Chains pngquant (lossy quantization) and oxipng (lossless optimization) for the best size-to-quality ratio.
  - JPEG: Uses mozjpeg for production-grade compression.

- **Modern Formats**: Optional generation of WebP and AVIF versions alongside the optimized originals.

- **Safety First**: Operates in "Safe Mode" by default, creating optimized copies without touching original files.

- **Flexible Input**: Supports processing entire directories (recursive), specific files, or lists of files (space or comma-separated).

- **Silent Mode**: Script-friendly mode with minimal output.

## 🛠 Usage

```
images-optimizer [OPTIONS] [PATHS]...
```

### Arguments

| Argument | Description |
|----------|-------------|
| `[PATHS]...` | The file(s) or directory to process. You can specify a single path, a space-separated list, or a comma-separated list of paths. |

### Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--replace` | | `false` | Destructive Mode. Overwrites original files in place. If not set, the tool runs in "Safe Mode" (see below). |
| `--webp` | | `false` | Generates a .webp version for every processed image. |
| `--avif` | | `false` | Generates an .avif version. Warning: This is extremely CPU intensive. |
| `--jpg-q` | | `80` | Quality setting for JPEG compression (0-100). |
| `--png-min` | | `65` | Minimum quality for PNG quantization (0-100). |
| `--png-max` | | `80` | Maximum quality for PNG quantization (0-100). |
| `--silent` | `-S` | `false` | Shows only the progress bar. Skips statistics and the "Press any key to exit" prompt. |
| `--help` | `-h` | `-` | Print help information. |
| `--version` | `-V` | `-` | Print version information. |

## 🧠 Modes & Behavior

### 1. Safe Mode (Default)

If the `--replace` flag is not provided, the tool ensures your original data remains untouched.

**Directory Input**:
- Example: `images-optimizer ./assets`
- Behavior: Creates a new directory named `./assets__optimized` next to the original. The entire folder structure is copied recursively, and optimization happens in the new folder.
- Note: If the output directory already exists, it is cleared before processing.

**Single File Input**:
- Example: `images-optimizer photo.jpg`
- Behavior: Creates a copy named `photo__optimized.jpg` in the same directory.
- Note: If `--webp` or `--avif` are used, they are generated based on the file name.

### 2. Replace Mode

If `--replace` is provided, the tool performs destructive optimization.

- Behavior: Images are compressed in place.
- WebP/AVIF: Generated alongside the original files (e.g., `image.jpg` -> `image.webp`).

### 3. Cumulative Time vs. Wall Time

In the final statistics, you will see two time metrics:

- **Wall time**: The actual time passed on the clock.
- **Cumulative Time**: The sum of time spent by all CPU cores. Since the tool is multi-threaded, Cumulative Time will often be higher than Wall Time (e.g., 10 seconds of Wall Time on an 8-core CPU might result in ~80 seconds of Cumulative work).

## 📋 Examples

1. **Optimize a folder safely (Recommended)**:
   ```bash
   images-optimizer ./my-gallery
   # Result: Creates "./my-gallery__optimized" with compressed images.
   ```

2. **Optimize specific files and overwrite them**:
   ```bash
   images-optimizer --replace photo1.jpg photo2.png
   ```

3. **Generate WebP and AVIF versions for a folder**:
   ```bash
   images-optimizer --webp --avif ./images
   ```

4. **High compression settings**:
   ```bash
   images-optimizer --jpg-q 60 --png-max 70 ./website-assets
   ```

5. **Silent execution (for scripts/CI)**:
   ```bash
   images-optimizer -S --replace ./assets
   ```
