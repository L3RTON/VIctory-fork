//! Every shader of the title translated before it runs, behind a loading
//! window: the containers in the title's files (its executable, DLLs and
//! archives) go through the same translation and on-disk cache the title's
//! pipelines use, so a pipeline made in the world finds its shaders already
//! translated instead of translating them while frames wait for it.
//!
//! A finished pass leaves a marker named by the cache key (our build and
//! the translation switches); later runs with the same key skip the pass.
//! Shaders the title builds or loads from elsewhere are still translated at
//! first use. `VICTORY_WARMUP=0` skips the pass, `VICTORY_WARMUP=force`
//! runs it even after a finished one. What does not translate is listed in
//! `shader-report.txt` in the shader cache folder.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;

use crate::shader::{MAX_FILE, sources};

/// Progress the loading window shows.
struct Progress {
    phase: Mutex<String>,
    total: AtomicUsize,
    done: AtomicUsize,
    failed: AtomicUsize,
    finished: AtomicBool,
}

static PROGRESS: Progress = Progress {
    phase: Mutex::new(String::new()),
    total: AtomicUsize::new(0),
    done: AtomicUsize::new(0),
    failed: AtomicUsize::new(0),
    finished: AtomicBool::new(false),
};

fn set_phase(text: impl Into<String>) {
    *PROGRESS.phase.lock().unwrap_or_else(|e| e.into_inner()) = text.into();
}

/// The files that may hold the title's shaders, smallest first.
fn shader_files(title_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<(u64, PathBuf)> = walkdir::WalkDir::new(title_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| ["exe", "dll", "rpf"].contains(&x.to_ascii_lowercase().as_str()))
        })
        .filter_map(|e| Some((e.metadata().ok()?.len(), e.into_path())))
        .filter(|(len, _)| *len <= MAX_FILE)
        .collect();
    files.sort();
    files.into_iter().map(|(_, p)| p).collect()
}

/// The container at the start of `data`, cut to its declared size.
fn container(data: &[u8]) -> Option<&[u8]> {
    let total = u32::from_le_bytes(data.get(0x18..0x1C)?.try_into().ok()?) as usize;
    data.get(..total)
}

/// The stages of a container's console programs (`Es+Gs`, `Ps`, ...);
/// `None` if it holds no console program (no shader code to translate).
fn stage(container: &[u8]) -> Option<String> {
    use victory_shader::{Container, FourCC};
    let c = Container::parse(container).ok()?;
    let programs = victory_shader::native_programs(c.part(FourCC::DXIL)?.data).ok()?;
    let stages: Vec<String> = programs.iter().map(|p| format!("{:?}", p.stage)).collect();
    (!stages.is_empty()).then(|| stages.join("+"))
}

/// What one container's translation came to.
enum Outcome {
    Translated { cached: bool },
    Failed { stage: String, error: String },
    /// No console program in it: not a shader.
    NoCode,
}

/// Translates one container through the title's own path (cache included).
fn translate_one(c: &[u8]) -> Outcome {
    let Some(stage) = stage(c) else {
        return Outcome::NoCode;
    };
    // A translator bug is one shader's failure, not the run's.
    match std::panic::catch_unwind(|| victory_gpu::shader_cache::warm(c)) {
        Ok(Ok(cached)) => Outcome::Translated { cached },
        Ok(Err(error)) => Outcome::Failed { stage, error },
        Err(_) => Outcome::Failed {
            stage,
            error: "the translator panicked".into(),
        },
    }
}

/// The reason of an error, without the code offset (they group better).
fn reason(error: &str) -> String {
    match error.split_once("GFX10 code at ") {
        Some((_, r)) => r.split_once(": ").map_or(r, |(_, r)| r).to_owned(),
        None => error.to_owned(),
    }
}

/// Keeps a copy of a container that does not translate, for checking it
/// again with `victory shaders --only` once the translator handles it.
fn keep_failed(cache_dir: &Path, stage: &str, hash: &str, c: &[u8]) {
    let dir = cache_dir.join("failed");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{}-{hash}.dxbc", stage.replace('+', "_"))), c);
}

/// `victory shaders --only`: translates the containers in `paths` and
/// reports each. Nothing is written but the cache entries of those that
/// translate (and copies of picked ones, below).
pub fn check(paths: &[PathBuf], cache_dir: &Path, validate: bool) -> Result<()> {
    if !victory_gpu::shader_cache::prepare_warm() {
        anyhow::bail!("no host GPU");
    }
    let mut validator = if validate {
        let dll = victory_shader::emit::validator::Validator::default_path();
        Some(victory_shader::emit::validator::Validator::load(dll.as_deref()).map_err(anyhow::Error::msg)?)
    } else {
        None
    };
    let mut invalid = 0;
    // `VICTORY_SHADER_HASHES=<hash>,...`: only containers whose 16-digit hash
    // starts with one of these; each is also copied to the cache's `picked`
    // folder. `VICTORY_SHADER_FRESH=1`: translated anew, past the cache (to see
    // what `VICTORY_WAVE_WHY=1` and the like print for it).
    let wanted: Vec<String> = std::env::var("VICTORY_SHADER_HASHES")
        .unwrap_or_default()
        .split(',')
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect();
    let fresh = std::env::var_os("VICTORY_SHADER_FRESH").is_some_and(|v| v == "1");
    let (mut ok, mut failed) = (0, 0);
    let mut seen = HashSet::<[u8; 32]>::new();
    for root in paths {
        for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(data) = std::fs::read(entry.path()) else { continue };
            for s in sources(&data) {
                let Some(bytes) = s.bytes() else { continue };
                let Some(c) = container(&bytes) else { continue };
                if !seen.insert(*blake3::hash(c).as_bytes()) {
                    continue;
                }
                let hash = blake3::hash(c).to_hex()[..16].to_owned();
                if !wanted.is_empty() {
                    if !wanted.iter().any(|w| hash.starts_with(w.as_str())) {
                        continue;
                    }
                    let dir = cache_dir.join("picked");
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(dir.join(format!("{hash}.dxbc")), c);
                }
                if let Some(v) = validator.as_mut() {
                    let Some(stage) = stage(c) else {
                        println!("none  {hash}  no console program");
                        continue;
                    };
                    match std::panic::catch_unwind(|| victory_shader::emit::translate(c)) {
                        Ok(Ok(shader)) => {
                            let mut dxil = shader.dxil.clone();
                            // A translation that keeps 64-lane waves on a host of
                            // 32 is refused when its pipeline is made: not done.
                            let lanes = victory_shader::emit::host_wave_lanes();
                            let wide = victory_shader::emit::required_wave(&shader.dxil)
                                .is_some_and(|w| lanes != 0 && w > lanes);
                            match v.validate(&mut dxil) {
                                Ok(()) if wide => {
                                    invalid += 1;
                                    println!("WAVE64 {hash}  {stage}  valid, but asks for waves wider than the host's {lanes}");
                                }
                                Ok(()) => {
                                    ok += 1;
                                    println!("ok    {hash}  {stage}  valid");
                                }
                                Err(e) => {
                                    invalid += 1;
                                    let e = e.replace('\n', " | ");
                                    println!("BAD   {hash}  {stage}  {e}");
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            failed += 1;
                            println!("FAIL  {hash}  {stage}  {e}");
                        }
                        Err(_) => {
                            failed += 1;
                            println!("FAIL  {hash}  {stage}  the translator panicked");
                        }
                    }
                    continue;
                }
                let outcome = if fresh {
                    match stage(c) {
                        None => Outcome::NoCode,
                        Some(stage) => match std::panic::catch_unwind(|| victory_shader::emit::translate(c)) {
                            Ok(Ok(_)) => Outcome::Translated { cached: false },
                            Ok(Err(e)) => Outcome::Failed { stage, error: e.to_string() },
                            Err(_) => Outcome::Failed { stage, error: "the translator panicked".into() },
                        },
                    }
                } else {
                    translate_one(c)
                };
                match outcome {
                    Outcome::Translated { .. } => {
                        ok += 1;
                        println!("ok    {hash}  {}", stage(c).unwrap_or_default());
                    }
                    Outcome::Failed { stage, error } => {
                        failed += 1;
                        println!("FAIL  {hash}  {stage}  {error}");
                    }
                    Outcome::NoCode => println!("none  {hash}  no console program"),
                }
            }
        }
    }
    if validator.is_some() {
        println!(
            "{ok} translate and validate, {invalid} translate but are invalid or need waves wider than the host's, {failed} do not translate"
        );
    } else {
        println!("{ok} translate, {failed} do not");
    }
    Ok(())
}

/// Translates every shader in the title's files into the shader cache,
/// showing a loading window while it runs. Call with the cache in use.
pub fn warm_shaders(title_dir: &Path, cache_dir: &Path) -> Result<()> {
    let switch = std::env::var("VICTORY_WARMUP").unwrap_or_default();
    if switch == "0" {
        return Ok(());
    }
    warm_shaders_with(title_dir, cache_dir, switch == "force")
}

/// [`warm_shaders`], `force` running the pass even after a finished one.
pub fn warm_shaders_with(title_dir: &Path, cache_dir: &Path, force: bool) -> Result<()> {
    if !victory_gpu::shader_cache::prepare_warm() {
        println!("shaders        no host GPU: not translated ahead of time");
        return Ok(());
    }
    let key = victory_gpu::shader_cache::cache_key();
    let marker = cache_dir.join(format!("warm-{key}.done"));
    if marker.exists() && !force {
        println!("shaders        translated ahead of time (cache {key})");
        return Ok(());
    }
    println!("shaders        translating every shader of the title first (cache {key})");
    let window = std::thread::spawn(window::run);
    let started = std::time::Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let seen = Mutex::new(HashSet::<[u8; 32]>::new());
    // Reason → (stage, shader hash, whole error) of each shader it stops.
    let failures = Mutex::new(BTreeMap::<String, Vec<(String, String, String)>>::new());
    let (mut translated, mut cached) = (0usize, 0usize);
    let no_code = AtomicUsize::new(0);
    let files = shader_files(title_dir);
    for (n, path) in files.iter().enumerate() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        set_phase(format!("Reading {name} ({} of {})", n + 1, files.len()));
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        let found = sources(&data);
        if found.is_empty() {
            continue;
        }
        set_phase(format!("Translating the shaders of {name} ({} of {})", n + 1, files.len()));
        // New containers of this file, unpacked once.
        let jobs: Vec<Vec<u8>> = found
            .iter()
            .filter_map(|s| {
                let bytes = s.bytes()?;
                let c = container(&bytes)?.to_vec();
                let hash = *blake3::hash(&c).as_bytes();
                seen.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(hash)
                    .then_some(c)
            })
            .collect();
        drop(found);
        PROGRESS.total.fetch_add(jobs.len(), Ordering::Relaxed);
        let next = AtomicUsize::new(0);
        let (fresh, hit) = (AtomicUsize::new(0), AtomicUsize::new(0));
        std::thread::scope(|s| {
            for _ in 0..threads.min(jobs.len()) {
                s.spawn(|| {
                    while let Some(c) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                        match translate_one(c) {
                            Outcome::Translated { cached: true } => _ = hit.fetch_add(1, Ordering::Relaxed),
                            Outcome::Translated { cached: false } => _ = fresh.fetch_add(1, Ordering::Relaxed),
                            Outcome::NoCode => _ = no_code.fetch_add(1, Ordering::Relaxed),
                            Outcome::Failed { stage, error } => {
                                PROGRESS.failed.fetch_add(1, Ordering::Relaxed);
                                let hash = blake3::hash(c).to_hex()[..16].to_owned();
                                keep_failed(cache_dir, &stage, &hash, c);
                                failures
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .entry(reason(&error))
                                    .or_default()
                                    .push((stage, hash, error));
                            }
                        }
                        PROGRESS.done.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
        translated += fresh.into_inner();
        cached += hit.into_inner();
    }
    let failures = failures.into_inner().unwrap_or_else(|e| e.into_inner());
    let failed: usize = failures.values().map(Vec::len).sum();
    let no_code = no_code.into_inner();
    let total = translated + cached + failed;
    let secs = started.elapsed().as_secs_f64();
    let mut report = format!(
        "cache {key}\n{total} shaders: {translated} translated now, {cached} already in the \
         cache, {failed} do not translate ({secs:.0} s); {no_code} containers hold no shader \
         code\nCopies of those that do not translate: {}\n\nReasons, by shaders stopped \
         (and by stage):\n",
        cache_dir.join("failed").display()
    );
    let mut reasons: Vec<_> = failures.iter().collect();
    reasons.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (reason, shaders) in &reasons {
        let mut stages = BTreeMap::<&str, usize>::new();
        for (stage, _, _) in shaders.iter() {
            *stages.entry(stage.as_str()).or_default() += 1;
        }
        let stages: Vec<String> = stages.iter().map(|(s, n)| format!("{s} {n}")).collect();
        report += &format!("{:6}  {reason}  [{}]\n", shaders.len(), stages.join(", "));
    }
    report += "\nShaders by reason (stage hash: error):\n";
    for (reason, shaders) in &reasons {
        report += &format!("{reason}:\n");
        for (stage, hash, error) in shaders.iter() {
            report += &format!("  {stage} {hash}: {error}\n");
        }
    }
    let report_path = cache_dir.join("shader-report.txt");
    let _ = std::fs::write(&report_path, report);
    println!(
        "shaders        {total}: {translated} translated, {cached} cached, {failed} do not \
         translate ({secs:.0} s; {})",
        report_path.display()
    );
    if no_code > 0 {
        println!("shaders        {no_code} containers hold no shader code");
    }
    let _ = std::fs::write(&marker, format!("{total} {failed}\n"));
    PROGRESS.finished.store(true, Ordering::Relaxed);
    let _ = window.join();
    Ok(())
}

/// The loading window: a progress bar and counts, repainted 10 times a
/// second until the pass is done.
mod window {
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::PROGRESS;

    const WIDTH: i32 = 760;
    const HEIGHT: i32 = 230;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        r as u32 | (g as u32) << 8 | (b as u32) << 16
    }

    pub(super) fn run() {
        let class = wide("VictoryShaderWarmup");
        let title = wide("VIctory - preparing shaders");
        // SAFETY: plain window creation and a message loop on this thread;
        // every pointer passed is a live local.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance,
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP | WS_VISIBLE | WS_BORDER,
                (sw - WIDTH) / 2,
                (sh - HEIGHT) / 2,
                WIDTH,
                HEIGHT,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return;
            }
            SetTimer(hwnd, 1, 100, None);
            let mut msg = std::mem::zeroed::<MSG>();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // SAFETY: the window's own messages, handled on its thread.
        unsafe {
            match msg {
                WM_TIMER => {
                    if PROGRESS.finished.load(Ordering::Relaxed) {
                        DestroyWindow(hwnd);
                    } else {
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                    }
                    0
                }
                WM_ERASEBKGND => 1,
                WM_PAINT => {
                    paint(hwnd);
                    0
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    0
                }
                _ => DefWindowProcW(hwnd, msg, w, l),
            }
        }
    }

    /// Draws off screen, then copies it to the window (no flicker).
    unsafe fn paint(hwnd: HWND) {
        // SAFETY: GDI calls on this window's paint DC and objects made here
        // and released before returning.
        unsafe {
            let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut area = RECT::default();
            GetClientRect(hwnd, &mut area);
            let mem = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, area.right, area.bottom);
            let old_bitmap = SelectObject(mem, bitmap);
            let fill = |r: RECT, color: u32| {
                let brush = CreateSolidBrush(color);
                FillRect(mem, &r, brush);
                DeleteObject(brush);
            };
            fill(area, rgb(18, 18, 24));
            let (total, done, failed) = (
                PROGRESS.total.load(Ordering::Relaxed),
                PROGRESS.done.load(Ordering::Relaxed),
                PROGRESS.failed.load(Ordering::Relaxed),
            );
            let face = wide("Segoe UI");
            let big = CreateFontW(34, 0, 0, 0, 600, 0, 0, 0, 0, 0, 0, 5, 0, face.as_ptr());
            let small = CreateFontW(20, 0, 0, 0, 400, 0, 0, 0, 0, 0, 0, 5, 0, face.as_ptr());
            SetBkMode(mem, 1); // transparent
            let text = |s: &str, font, color, top: i32, h: i32| {
                SelectObject(mem, font);
                SetTextColor(mem, color);
                let mut r = RECT {
                    left: 30,
                    top,
                    right: area.right - 30,
                    bottom: top + h,
                };
                let s: Vec<u16> = s.encode_utf16().collect();
                DrawTextW(mem, s.as_ptr(), s.len() as i32, &mut r, DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
            };
            let old_font = SelectObject(mem, big);
            text("Preparing shaders", big, rgb(240, 240, 245), 24, 40);
            let phase = PROGRESS.phase.lock().map(|p| p.clone()).unwrap_or_default();
            text(&phase, small, rgb(170, 170, 185), 70, 26);
            let bar = RECT {
                left: 30,
                top: 112,
                right: area.right - 30,
                bottom: 138,
            };
            fill(bar, rgb(45, 45, 58));
            if total > 0 {
                let width = ((bar.right - bar.left) as f64 * done as f64 / total as f64) as i32;
                fill(RECT { right: bar.left + width, ..bar }, rgb(90, 170, 250));
            }
            let pct = if total > 0 { done * 100 / total } else { 0 };
            text(
                &format!(
                    "{done} of {total} shaders ({pct}%)   translated {}   not translatable {failed}",
                    done - failed.min(done)
                ),
                small,
                rgb(220, 220, 230),
                152,
                26,
            );
            text(
                "The game starts when every shader is ready. Later runs skip this step.",
                small,
                rgb(130, 130, 145),
                182,
                26,
            );
            BitBlt(dc, 0, 0, area.right, area.bottom, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old_font);
            SelectObject(mem, old_bitmap);
            DeleteObject(big);
            DeleteObject(small);
            DeleteObject(bitmap);
            DeleteDC(mem);
            EndPaint(hwnd, &ps);
        }
    }
}
