use anyhow::{bail, Context, Result};
use common::{thumbs_dir, wallpapers_dir, WallpaperItem, WallpaperKind};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Locate ffmpeg.exe or ffprobe.exe next to executable or in deps folder.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };

    // 1. Next to current executable
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let candidate = parent.join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 2. In deps directory
    let deps_candidate = PathBuf::from("deps").join(&exe_name);
    if deps_candidate.exists() {
        return Some(deps_candidate);
    }

    // 3. In target debug/release
    let target_debug = PathBuf::from("target/debug").join(&exe_name);
    if target_debug.exists() {
        return Some(target_debug);
    }

    // 4. In PATH
    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Metadata extracted via ffprobe.
#[derive(Debug, Clone)]
pub struct MediaProbe {
    pub kind: WallpaperKind,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_ms: u64,
    pub is_animated: bool,
}

#[derive(Deserialize)]
struct FFprobeOutput {
    streams: Option<Vec<FFprobeStream>>,
    format: Option<FFprobeFormat>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FFprobeStream {
    codec_name: Option<String>,
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    nb_frames: Option<String>,
    duration: Option<String>,
}

#[derive(Deserialize)]
struct FFprobeFormat {
    duration: Option<String>,
}

/// Run ffprobe on file to determine media type and parameters.
pub fn probe_media(file_path: &Path) -> Result<MediaProbe> {
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "html" || ext == "htm" {
        return Ok(MediaProbe {
            kind: WallpaperKind::Html,
            codec: "html5".to_string(),
            width: 1920,
            height: 1080,
            fps: 60.0,
            duration_ms: 0,
            is_animated: false,
        });
    }

    if ext == "wgsl" {
        return Ok(MediaProbe {
            kind: WallpaperKind::Shader,
            codec: "wgsl".to_string(),
            width: 1920,
            height: 1080,
            fps: 60.0,
            duration_ms: 0,
            is_animated: false,
        });
    }

    let ffprobe = find_tool("ffprobe").context("ffprobe tool not found in project or PATH")?;

    let output = Command::new(ffprobe)
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(file_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("Failed to execute ffprobe")?;

    if !output.status.success() {
        bail!("ffprobe failed on file: {}", file_path.display());
    }

    let parsed: FFprobeOutput =
        serde_json::from_slice(&output.stdout).context("Failed to parse ffprobe JSON output")?;

    let video_stream = parsed
        .streams
        .as_ref()
        .and_then(|s| s.iter().find(|st| st.codec_type.as_deref() == Some("video")));

    let width = video_stream.and_then(|s| s.width).unwrap_or(1920);
    let height = video_stream.and_then(|s| s.height).unwrap_or(1080);
    let codec = video_stream
        .and_then(|s| s.codec_name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let mut duration_ms = 0u64;
    if let Some(dur_str) = parsed.format.as_ref().and_then(|f| f.duration.as_deref()) {
        if let Ok(sec) = dur_str.parse::<f64>() {
            duration_ms = (sec * 1000.0).round() as u64;
        }
    }

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let is_animated = match ext.as_str() {
        "gif" | "apng" => true,
        "webp" => duration_ms > 0,
        _ => false,
    };

    let kind = match ext.as_str() {
        "mp4" | "webm" | "mkv" | "mov" | "avi" => WallpaperKind::Video,
        "gif" | "apng" => WallpaperKind::Animated,
        "webp" if is_animated => WallpaperKind::Animated,
        _ => WallpaperKind::Image,
    };

    let mut fps = 30.0;
    if let Some(fps_str) = video_stream.and_then(|s| s.r_frame_rate.as_deref()) {
        let parts: Vec<&str> = fps_str.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(num), Ok(den)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                if den > 0.0 {
                    fps = num / den;
                }
            }
        }
    }

    Ok(MediaProbe {
        kind,
        codec,
        width,
        height,
        fps,
        duration_ms,
        is_animated,
    })
}

/// Generate a WebP thumbnail around 320px width.
pub fn generate_thumbnail(input: &Path, output: &Path) -> Result<()> {
    let ffmpeg = find_tool("ffmpeg").context("ffmpeg tool not found in project or PATH")?;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "html" || ext == "htm" || ext == "wgsl" {
        let col = if ext == "wgsl" { "0x8B5CF6" } else { "0x06B6D4" };
        let status = Command::new(&ffmpeg)
            .arg("-y")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg(format!("color=c={}:s=320x180:d=1", col))
            .arg("-vframes")
            .arg("1")
            .arg(output)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if let Ok(s) = status {
            if s.success() {
                return Ok(());
            }
        }
    }

    let status = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-ss")
        .arg("00:00:00.500")
        .arg("-i")
        .arg(input)
        .arg("-vf")
        .arg("scale=320:-1")
        .arg("-vframes")
        .arg("1")
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to execute ffmpeg for thumbnail generation")?;

    if !status.success() {
        // Fallback without -ss if media is very short
        let status2 = Command::new(ffmpeg)
            .arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-vf")
            .arg("scale=320:-1")
            .arg("-vframes")
            .arg("1")
            .arg(output)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Failed to execute ffmpeg fallback thumbnail generation")?;

        if !status2.success() {
            bail!("Failed to generate thumbnail for: {}", input.display());
        }
    }

    Ok(())
}

/// Convert GIF/APNG/animated WebP into MP4 (H.264, yuv420p) for optimal HWDEC.
pub fn transcode_animated_to_mp4(input: &Path, output: &Path) -> Result<()> {
    let ffmpeg = find_tool("ffmpeg").context("ffmpeg tool not found in project or PATH")?;

    tracing::info!(
        "Transcoding animated media {} -> {}",
        input.display(),
        output.display()
    );

    let status = Command::new(ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-movflags")
        .arg("faststart")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-vf")
        .arg("scale=trunc(iw/2)*2:trunc(ih/2)*2")
        .arg("-c:v")
        .arg("libx264")
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to execute ffmpeg transcoding")?;

    if !status.success() {
        bail!("FFmpeg transcoding failed for {}", input.display());
    }

    Ok(())
}

/// Import a wallpaper file, generating metadata, thumbnail, and transcoding if required.
pub fn import_wallpaper(source: &Path, copy_to_lib: bool) -> Result<WallpaperItem> {
    if !source.exists() {
        bail!("Source file does not exist: {}", source.display());
    }

    let probe = probe_media(source).context("Failed to probe source media")?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64;

    let id = format!("wp_{}", now);
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("wallpaper");

    let thumbs_path = thumbs_dir().join(format!("{}.webp", id));
    generate_thumbnail(source, &thumbs_path).context("Failed to create wallpaper thumbnail")?;

    let processed_path = if probe.is_animated {
        // Transcode animated GIF/APNG/WebP into MP4
        let mp4_dest = wallpapers_dir().join(format!("{}.mp4", id));
        transcode_animated_to_mp4(source, &mp4_dest)?;
        mp4_dest
    } else if copy_to_lib {
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("dat");
        let lib_dest = wallpapers_dir().join(format!("{}.{}", id, ext));
        fs::copy(source, &lib_dest)
            .with_context(|| format!("Failed to copy {} to library", source.display()))?;
        lib_dest
    } else {
        source.to_path_buf()
    };

    let final_kind = if probe.is_animated {
        WallpaperKind::Video.as_str().to_string()
    } else {
        probe.kind.as_str().to_string()
    };

    Ok(WallpaperItem {
        id,
        name: stem.to_string(),
        kind: final_kind,
        source_path: source.to_string_lossy().to_string(),
        processed_path: processed_path.to_string_lossy().to_string(),
        thumb_path: thumbs_path.to_string_lossy().to_string(),
        fit: "fill".to_string(),
        fps_cap: if probe.fps > 30.0 { Some(30) } else { None },
        width: Some(probe.width),
        height: Some(probe.height),
        duration_ms: if probe.duration_ms > 0 {
            Some(probe.duration_ms)
        } else {
            None
        },
        created_at: now,
    })
}
