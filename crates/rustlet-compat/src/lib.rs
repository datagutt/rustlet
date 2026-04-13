use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage, ImageBuffer, RgbaImage};
use rustlet_encode::{self as encode, OutputFormat as EncodeOutputFormat};
use rustlet_runtime::Applet;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tiny_skia::Pixmap;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    Fixture,
    ReferenceApp,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonPolicy {
    Exact,
    KnownDiff,
    ExpectedFail,
    Skip,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Webp,
    Gif,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompatCase {
    pub id: String,
    pub kind: CaseKind,
    pub path: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub double: bool,
    #[serde(default)]
    pub max_frames: Option<usize>,
    #[serde(default = "default_policy")]
    pub policy: ComparisonPolicy,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub requires_live_data: bool,
    #[serde(default)]
    pub requires_network: bool,
    #[serde(default = "default_output_format")]
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompatManifest {
    pub cases: Vec<CompatCase>,
}

#[derive(Debug, Clone)]
pub struct NormalizedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct NormalizedRun {
    pub status: RunStatus,
    pub width: u32,
    pub height: u32,
    pub frame_delay_ms: u32,
    pub frames: Vec<NormalizedFrame>,
}

#[derive(Debug, Clone)]
pub enum RunStatus {
    Success,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ComparisonFailure {
    pub kind: FailureKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureKind {
    RuntimeErrorMismatch,
    FrameCountMismatch,
    DelayMismatch,
    DimensionMismatch,
    PixelMismatch,
}

#[derive(Debug, Clone)]
pub enum CaseOutcome {
    ExactMatch,
    KnownDiff(ComparisonFailure),
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct CaseReport {
    pub case: CompatCase,
    pub outcome: CaseOutcome,
}

#[derive(Debug, Deserialize)]
struct WebpDump {
    width: u32,
    height: u32,
    delays_ms: Vec<u32>,
    frames: Vec<WebpDumpFrame>,
}

#[derive(Debug, Deserialize)]
struct WebpDumpFrame {
    rgba_hex: String,
}

pub fn load_manifest(crate_root: &Path) -> Result<CompatManifest> {
    let path = crate_root.join("compat_cases/cases.json");
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse manifest {}", path.display()))
}

pub fn run_case(workspace_root: &Path, crate_root: &Path, case: CompatCase) -> Result<CaseReport> {
    if case.policy == ComparisonPolicy::Skip {
        return Ok(CaseReport {
            case: case.clone(),
            outcome: CaseOutcome::Skipped(case.reason.clone().unwrap_or_else(|| "skipped".into())),
        });
    }

    if case.requires_live_data && env::var_os("RUSTLET_COMPAT_ALLOW_LIVE_DATA").is_none() {
        return Ok(CaseReport {
            case: case.clone(),
            outcome: CaseOutcome::Skipped(
                "requires live data; set RUSTLET_COMPAT_ALLOW_LIVE_DATA=1".into(),
            ),
        });
    }

    if case.requires_network && env::var_os("RUSTLET_COMPAT_ALLOW_NETWORK").is_none() {
        return Ok(CaseReport {
            case: case.clone(),
            outcome: CaseOutcome::Skipped(
                "requires network; set RUSTLET_COMPAT_ALLOW_NETWORK=1".into(),
            ),
        });
    }

    let rustlet = run_rustlet_case(workspace_root, crate_root, &case)?;
    let pixlet = run_pixlet_case(workspace_root, crate_root, &case)?;
    let comparison = compare_runs(&case, &rustlet, &pixlet);
    let artifacts_root = artifacts_root(workspace_root);

    match case.policy {
        ComparisonPolicy::Exact => match comparison {
            None => Ok(CaseReport {
                case,
                outcome: CaseOutcome::ExactMatch,
            }),
            Some(failure) => {
                write_artifacts(&artifacts_root, &case.id, &failure, &rustlet, &pixlet)?;
                Err(anyhow!("{}: {}", case.id, failure.message))
            }
        },
        ComparisonPolicy::KnownDiff => {
            if let Some(failure) = comparison {
                write_artifacts(&artifacts_root, &case.id, &failure, &rustlet, &pixlet)?;
                Ok(CaseReport {
                    case,
                    outcome: CaseOutcome::KnownDiff(failure),
                })
            } else {
                Ok(CaseReport {
                    case,
                    outcome: CaseOutcome::ExactMatch,
                })
            }
        }
        ComparisonPolicy::ExpectedFail => match (&rustlet.status, &pixlet.status) {
            (RunStatus::Error(_), RunStatus::Error(_)) => Ok(CaseReport {
                case,
                outcome: CaseOutcome::ExactMatch,
            }),
            _ => {
                let failure = ComparisonFailure {
                    kind: FailureKind::RuntimeErrorMismatch,
                    message: format!("{}: expected both engines to fail", case.id),
                };
                write_artifacts(&artifacts_root, &case.id, &failure, &rustlet, &pixlet)?;
                Err(anyhow!(failure.message))
            }
        },
        ComparisonPolicy::Skip => unreachable!(),
    }
}

pub fn pixlet_available(workspace_root: &Path) -> bool {
    resolve_pixlet_binary(workspace_root).is_ok() && build_webp_dump_binary(workspace_root).is_ok()
}

fn artifacts_root(workspace_root: &Path) -> PathBuf {
    if let Some(path) = env::var_os("RUSTLET_COMPAT_ARTIFACTS_DIR") {
        return PathBuf::from(path);
    }

    let workspace_target = workspace_root.join("target/compat-artifacts");
    if fs::create_dir_all(&workspace_target).is_ok()
        && tempfile::NamedTempFile::new_in(&workspace_target).is_ok()
    {
        return workspace_target;
    }

    std::env::temp_dir().join("rustlet-compat-artifacts")
}

fn default_width() -> u32 {
    64
}

fn default_height() -> u32 {
    32
}

fn default_policy() -> ComparisonPolicy {
    ComparisonPolicy::Exact
}

fn default_output_format() -> OutputFormat {
    OutputFormat::Webp
}

fn run_rustlet_case(workspace_root: &Path, crate_root: &Path, case: &CompatCase) -> Result<NormalizedRun> {
    let resolved = resolve_case_path(workspace_root, crate_root, case)?;
    let entry = resolve_entry_file(&resolved)?;
    let src = fs::read_to_string(&entry)
        .with_context(|| format!("failed to read {}", entry.display()))?;
    let base_dir = entry.parent();
    let applet = Applet::new();

    let roots = match applet.run_with_options(
        entry.file_name().and_then(|s| s.to_str()).unwrap_or("main.star"),
        &src,
        &case.config,
        case.width,
        case.height,
        case.double,
        base_dir,
    ) {
        Ok(roots) => roots,
        Err(err) => {
            return Ok(NormalizedRun {
                status: RunStatus::Error(err.to_string()),
                width: effective_width(case),
                height: effective_height(case),
                frame_delay_ms: 0,
                frames: Vec::new(),
            });
        }
    };

    let mut pixmaps = Vec::<Pixmap>::new();
    let mut delay_ms = 50u32;
    for (index, root) in roots.iter().enumerate() {
        if index == 0 {
            delay_ms = root.delay.max(0) as u32;
        }
        pixmaps.extend(root.paint_frames(effective_width(case), effective_height(case)));
        if let Some(limit) = case.max_frames {
            if pixmaps.len() >= limit {
                pixmaps.truncate(limit);
                break;
            }
        }
    }

    let bytes = encode::encode(
        &pixmaps,
        delay_ms as u16,
        match case.output_format {
            OutputFormat::Gif => EncodeOutputFormat::Gif,
            OutputFormat::Webp => EncodeOutputFormat::WebP,
        },
    )
    .with_context(|| format!("failed to encode rustlet output for {}", case.id))?;

    decode_encoded_run(workspace_root, &bytes, case)
}

fn run_pixlet_case(workspace_root: &Path, crate_root: &Path, case: &CompatCase) -> Result<NormalizedRun> {
    let pixlet = resolve_pixlet_binary(workspace_root)?;
    let case_path = resolve_case_path(workspace_root, crate_root, case)?;
    let temp = tempdir().context("failed to create temp dir for pixlet output")?;
    let ext = match case.output_format {
        OutputFormat::Webp => "webp",
        OutputFormat::Gif => "gif",
    };
    let output_path = temp.path().join(format!("{}.{}", case.id, ext));
    let config_path = temp.path().join("config.json");
    if !case.config.is_empty() {
        let json = serde_json::to_vec(&case.config).context("failed to serialize config JSON")?;
        fs::write(&config_path, json).context("failed to write config JSON")?;
    }

    let mut cmd = Command::new(&pixlet);
    cmd.current_dir(workspace_root.join(".reference/pixlet"))
        .arg("render")
        .arg(case_path.as_os_str())
        .arg("--width")
        .arg(case.width.to_string())
        .arg("--height")
        .arg(case.height.to_string())
        .arg("--output")
        .arg(&output_path)
        .arg("--format")
        .arg(match case.output_format {
            OutputFormat::Webp => "webp",
            OutputFormat::Gif => "gif",
        })
        .arg("--silent")
        .arg("--timeout")
        .arg("30s");

    if case.double {
        cmd.arg("--2x");
    }
    if !case.config.is_empty() {
        cmd.arg("--config").arg(&config_path);
    }

    let output = cmd.output().context("failed to run pixlet render")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Ok(NormalizedRun {
            status: RunStatus::Error(detail),
            width: effective_width(case),
            height: effective_height(case),
            frame_delay_ms: 0,
            frames: Vec::new(),
        });
    }

    let bytes = fs::read(&output_path)
        .with_context(|| format!("failed to read pixlet output {}", output_path.display()))?;
    decode_encoded_run(workspace_root, &bytes, case)
}

fn decode_encoded_run(workspace_root: &Path, bytes: &[u8], case: &CompatCase) -> Result<NormalizedRun> {
    match case.output_format {
        OutputFormat::Gif => decode_gif_run(bytes, case),
        OutputFormat::Webp => decode_webp_run(workspace_root, bytes, case),
    }
}

fn decode_gif_run(bytes: &[u8], case: &CompatCase) -> Result<NormalizedRun> {
    let decoder = GifDecoder::new(std::io::Cursor::new(bytes)).context("failed to decode GIF")?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .context("failed to read GIF frames")?;
    let mut normalized = Vec::new();
    let mut delay_ms = 0u32;
    for frame in frames {
        if delay_ms == 0 {
            let (num, denom) = frame.delay().numer_denom_ms();
            delay_ms = if denom == 0 { 0 } else { num / denom.max(1) };
        }
        normalized.push(normalize_rgba_image(&DynamicImage::ImageRgba8(frame.into_buffer())));
        if let Some(limit) = case.max_frames {
            if normalized.len() >= limit {
                break;
            }
        }
    }
    Ok(NormalizedRun {
        status: RunStatus::Success,
        width: effective_width(case),
        height: effective_height(case),
        frame_delay_ms: delay_ms,
        frames: normalized,
    })
}

fn decode_webp_run(workspace_root: &Path, bytes: &[u8], case: &CompatCase) -> Result<NormalizedRun> {
    let temp = tempdir().context("failed to create temp dir for webp decode")?;
    let input = temp.path().join("frame.webp");
    fs::write(&input, bytes).context("failed to write temp webp")?;
    let helper = build_webp_dump_binary(workspace_root)?;
    let output = Command::new(helper)
        .arg(&input)
        .output()
        .context("failed to run webp decoder helper")?;
    if !output.status.success() {
        bail!(
            "webp decoder helper failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let decoded: WebpDump =
        serde_json::from_slice(&output.stdout).context("failed to parse webp decoder output")?;
    let mut frames = Vec::new();
    for frame in decoded.frames {
        let rgba = decode_hex(&frame.rgba_hex).context("invalid hex-encoded RGBA frame")?;
        frames.push(NormalizedFrame {
            width: decoded.width,
            height: decoded.height,
            hash: hash_bytes(&rgba),
            rgba,
        });
        if let Some(limit) = case.max_frames {
            if frames.len() >= limit {
                break;
            }
        }
    }
    Ok(NormalizedRun {
        status: RunStatus::Success,
        width: decoded.width,
        height: decoded.height,
        frame_delay_ms: decoded.delays_ms.first().copied().unwrap_or(0),
        frames,
    })
}

fn compare_runs(case: &CompatCase, rustlet: &NormalizedRun, pixlet: &NormalizedRun) -> Option<ComparisonFailure> {
    match (&rustlet.status, &pixlet.status) {
        (RunStatus::Error(lhs), RunStatus::Error(rhs)) => {
            if lhs == rhs || case.policy == ComparisonPolicy::ExpectedFail {
                return None;
            }
            return Some(ComparisonFailure {
                kind: FailureKind::RuntimeErrorMismatch,
                message: format!("{}: both failed differently\nrustlet: {lhs}\npixlet: {rhs}", case.id),
            });
        }
        (RunStatus::Error(lhs), RunStatus::Success) => {
            return Some(ComparisonFailure {
                kind: FailureKind::RuntimeErrorMismatch,
                message: format!("{}: rustlet failed but pixlet succeeded: {lhs}", case.id),
            });
        }
        (RunStatus::Success, RunStatus::Error(rhs)) => {
            return Some(ComparisonFailure {
                kind: FailureKind::RuntimeErrorMismatch,
                message: format!("{}: pixlet failed but rustlet succeeded: {rhs}", case.id),
            });
        }
        (RunStatus::Success, RunStatus::Success) => {}
    }

    if rustlet.width != pixlet.width || rustlet.height != pixlet.height {
        return Some(ComparisonFailure {
            kind: FailureKind::DimensionMismatch,
            message: format!(
                "{}: dimensions differ rustlet={}x{} pixlet={}x{}",
                case.id, rustlet.width, rustlet.height, pixlet.width, pixlet.height
            ),
        });
    }

    if rustlet.frame_delay_ms != pixlet.frame_delay_ms {
        return Some(ComparisonFailure {
            kind: FailureKind::DelayMismatch,
            message: format!(
                "{}: frame delay differs rustlet={}ms pixlet={}ms",
                case.id, rustlet.frame_delay_ms, pixlet.frame_delay_ms
            ),
        });
    }

    if rustlet.frames.len() != pixlet.frames.len() {
        return Some(ComparisonFailure {
            kind: FailureKind::FrameCountMismatch,
            message: format!(
                "{}: frame count differs rustlet={} pixlet={}",
                case.id,
                rustlet.frames.len(),
                pixlet.frames.len()
            ),
        });
    }

    for (index, (lhs, rhs)) in rustlet.frames.iter().zip(&pixlet.frames).enumerate() {
        if lhs.width != rhs.width || lhs.height != rhs.height {
            return Some(ComparisonFailure {
                kind: FailureKind::DimensionMismatch,
                message: format!(
                    "{}: frame {index} dimensions differ rustlet={}x{} pixlet={}x{}",
                    case.id, lhs.width, lhs.height, rhs.width, rhs.height
                ),
            });
        }
        if lhs.hash != rhs.hash || lhs.rgba != rhs.rgba {
            return Some(ComparisonFailure {
                kind: FailureKind::PixelMismatch,
                message: format!("{}: pixel mismatch at frame {index}", case.id),
            });
        }
    }

    None
}

fn write_artifacts(
    artifacts_root: &Path,
    case_id: &str,
    failure: &ComparisonFailure,
    rustlet: &NormalizedRun,
    pixlet: &NormalizedRun,
) -> Result<()> {
    let dir = artifacts_root.join(case_id);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create artifact dir {}", dir.display()))?;

    let summary = serde_json::json!({
        "kind": format!("{:?}", failure.kind),
        "message": failure.message,
        "rustlet": summarize_run(rustlet),
        "pixlet": summarize_run(pixlet),
    });
    fs::write(dir.join("summary.json"), serde_json::to_vec_pretty(&summary)?)
        .context("failed to write summary artifact")?;

    if let (Some(lhs), Some(rhs)) = (rustlet.frames.first(), pixlet.frames.first()) {
        write_frame_png(&dir.join("rustlet_frame_0.png"), lhs)?;
        write_frame_png(&dir.join("pixlet_frame_0.png"), rhs)?;
        let diff = make_diff_image(lhs, rhs);
        diff.save(dir.join("diff_frame_0.png"))
            .context("failed to write diff artifact")?;
    }

    Ok(())
}

fn summarize_run(run: &NormalizedRun) -> serde_json::Value {
    serde_json::json!({
        "status": match &run.status {
            RunStatus::Success => "success".to_string(),
            RunStatus::Error(err) => err.clone(),
        },
        "width": run.width,
        "height": run.height,
        "frame_delay_ms": run.frame_delay_ms,
        "frame_count": run.frames.len(),
    })
}

fn write_frame_png(path: &Path, frame: &NormalizedFrame) -> Result<()> {
    let img = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .ok_or_else(|| anyhow!("failed to build RGBA image buffer"))?;
    img.save(path)
        .with_context(|| format!("failed to save {}", path.display()))
}

fn make_diff_image(lhs: &NormalizedFrame, rhs: &NormalizedFrame) -> RgbaImage {
    let width = lhs.width.min(rhs.width);
    let height = lhs.height.min(rhs.height);
    let mut out = ImageBuffer::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let same = lhs.rgba.get(idx..idx + 4) == rhs.rgba.get(idx..idx + 4);
            let pixel = if same {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 0, 255, 255])
            };
            out.put_pixel(x, y, pixel);
        }
    }
    out
}

fn normalize_rgba_image(img: &DynamicImage) -> NormalizedFrame {
    let rgba = img.to_rgba8();
    let bytes = rgba.as_raw().clone();
    NormalizedFrame {
        width: rgba.width(),
        height: rgba.height(),
        hash: hash_bytes(&bytes),
        rgba: bytes,
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn effective_width(case: &CompatCase) -> u32 {
    if case.double { 128 } else { case.width }
}

fn effective_height(case: &CompatCase) -> u32 {
    if case.double { 64 } else { case.height }
}

fn resolve_case_path(workspace_root: &Path, crate_root: &Path, case: &CompatCase) -> Result<PathBuf> {
    let path = Path::new(&case.path);
    let resolved = match case.kind {
        CaseKind::Fixture => crate_root.join("compat_cases").join(path),
        CaseKind::ReferenceApp => workspace_root.join(path),
    };
    if resolved.exists() {
        Ok(resolved)
    } else {
        bail!("case path does not exist: {}", resolved.display())
    }
}

fn resolve_entry_file(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        if let Some(star) = fs::read_dir(path)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .find(|candidate| candidate.extension().and_then(|s| s.to_str()) == Some("star"))
        {
            Ok(star)
        } else {
            bail!("no .star file found in {}", path.display())
        }
    } else {
        Ok(path.to_path_buf())
    }
}

fn build_pixlet_binary(workspace_root: &Path) -> Result<PathBuf> {
    let tools_dir = workspace_root.join("target/compat-tools");
    fs::create_dir_all(&tools_dir).context("failed to create target/compat-tools")?;
    let gopath = tools_dir.join("go-path");
    let gomodcache = tools_dir.join("go-mod-cache");
    fs::create_dir_all(&gopath).context("failed to create go-path cache")?;
    fs::create_dir_all(&gomodcache).context("failed to create go-mod-cache")?;
    let binary = tools_dir.join(if cfg!(windows) { "pixlet.exe" } else { "pixlet" });
    if binary.exists() && env::var_os("RUSTLET_COMPAT_BUILD_PIXLET").is_none() {
        return Ok(binary);
    }

    let status = Command::new("go")
        .current_dir(workspace_root.join(".reference/pixlet"))
        .env("GOPATH", &gopath)
        .env("GOMODCACHE", &gomodcache)
        .arg("build")
        .arg("-o")
        .arg(&binary)
        .arg(".")
        .status()
        .context("failed to invoke go build for pixlet")?;
    if !status.success() {
        bail!("go build failed for vendored pixlet");
    }
    Ok(binary)
}

fn resolve_pixlet_binary(workspace_root: &Path) -> Result<PathBuf> {
    if let Some(path) = env::var_os("RUSTLET_COMPAT_PIXLET") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    if let Some(path) = find_path_binary("pixlet") {
        return Ok(path);
    }

    build_pixlet_binary(workspace_root)
}

fn build_webp_dump_binary(workspace_root: &Path) -> Result<PathBuf> {
    let tools_dir = workspace_root.join("target/compat-tools");
    fs::create_dir_all(&tools_dir).context("failed to create target/compat-tools")?;
    let gopath = tools_dir.join("go-path");
    let gomodcache = tools_dir.join("go-mod-cache");
    fs::create_dir_all(&gopath).context("failed to create go-path cache")?;
    fs::create_dir_all(&gomodcache).context("failed to create go-mod-cache")?;
    let binary = tools_dir.join(if cfg!(windows) { "webp-dump.exe" } else { "webp-dump" });
    if binary.exists() && env::var_os("RUSTLET_COMPAT_BUILD_PIXLET").is_none() {
        return Ok(binary);
    }

    let src_dir = tools_dir.join("webp-dump-src");
    fs::create_dir_all(&src_dir).context("failed to create webp-dump-src")?;
    fs::write(src_dir.join("go.mod"), WEBP_DUMP_GO_MOD)
        .context("failed to write webp dump go.mod")?;
    fs::write(src_dir.join("main.go"), WEBP_DUMP_MAIN_GO)
        .context("failed to write webp dump main.go")?;

    let tidy_status = Command::new("go")
        .current_dir(&src_dir)
        .env("GOPATH", &gopath)
        .env("GOMODCACHE", &gomodcache)
        .arg("mod")
        .arg("tidy")
        .status()
        .context("failed to invoke go mod tidy for webp dump helper")?;
    if !tidy_status.success() {
        bail!("go mod tidy failed for webp dump helper");
    }

    let status = Command::new("go")
        .current_dir(&src_dir)
        .env("GOPATH", &gopath)
        .env("GOMODCACHE", &gomodcache)
        .arg("build")
        .arg("-o")
        .arg(&binary)
        .arg(".")
        .status()
        .context("failed to invoke go build for webp dump helper")?;
    if !status.success() {
        bail!("go build failed for webp dump helper");
    }
    Ok(binary)
}

fn decode_hex(input: &str) -> Result<Vec<u8>> {
    if input.len() % 2 != 0 {
        bail!("hex input has odd length");
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        out.push((hex_nibble(bytes[idx])? << 4) | hex_nibble(bytes[idx + 1])?);
        idx += 2;
    }
    Ok(out)
}

fn find_path_binary(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate_exe = dir.join(format!("{name}.exe"));
            if candidate_exe.is_file() {
                return Some(candidate_exe);
            }
        }
    }
    None
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex digit"),
    }
}

const WEBP_DUMP_GO_MOD: &str = r#"module rustlet-compat/webp-dump

go 1.26.2

require github.com/tronbyt/go-libwebp v0.0.0-20251221160926-0c04b4a7738a
"#;

const WEBP_DUMP_MAIN_GO: &str = r#"package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"

	"github.com/tronbyt/go-libwebp/webp"
)

type frame struct {
	RGBAHex string `json:"rgba_hex"`
}

type dump struct {
	Width    uint32   `json:"width"`
	Height   uint32   `json:"height"`
	DelaysMS []uint32 `json:"delays_ms"`
	Frames   []frame  `json:"frames"`
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: webp-dump <file>")
		os.Exit(2)
	}

	data, err := os.ReadFile(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	decoder, err := webp.NewAnimationDecoder(data)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	img, err := decoder.Decode()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	out := dump{
		DelaysMS: make([]uint32, 0, len(img.Timestamp)),
		Frames:   make([]frame, 0, len(img.Image)),
	}
	last := 0
	for _, ts := range img.Timestamp {
		out.DelaysMS = append(out.DelaysMS, uint32(ts-last))
		last = ts
	}
	for i, im := range img.Image {
		bounds := im.Bounds()
		if i == 0 {
			out.Width = uint32(bounds.Dx())
			out.Height = uint32(bounds.Dy())
		}
		rgba := make([]byte, 0, bounds.Dx()*bounds.Dy()*4)
		for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
			for x := bounds.Min.X; x < bounds.Max.X; x++ {
				r, g, b, a := im.At(x, y).RGBA()
				rgba = append(rgba, uint8(r>>8), uint8(g>>8), uint8(b>>8), uint8(a>>8))
			}
		}
		out.Frames = append(out.Frames, frame{RGBAHex: hex.EncodeToString(rgba)})
	}

	if err := json.NewEncoder(os.Stdout).Encode(out); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
"#;
