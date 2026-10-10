//! `vectorizer` – command-line and interactive studio front end for `vectorizer-core`.
//!
//! ```text
//! vectorizer [gui|studio]                               launch interactive desktop Studio UI
//! vectorizer convert <files-or-folders...> [options]   batch convert (parallel)
//! vectorizer demo [outdir]                              render synthetic scenes and vectorize them
//! vectorizer bench <image> [--runs N] [options]        timing of the pipeline stages
//! ```

mod server;

use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use vectorizer_core::testing;
use vectorizer_core::{
    vectorize, DxfOptions, DxfUnits, Params, RasterImage, SvgOptions, TopologyMode,
};

const USAGE: &str = "\
vectorizer - classical geometric raster-to-vector engine

USAGE:
  vectorizer [gui|studio]
  vectorizer convert <files-or-folders...> [options]
  vectorizer demo [outdir]
  vectorizer bench <image> [--runs N] [options]

OPTIONS:
  -o, --out <dir>         output directory (default: next to each input)
  -f, --formats <list>    comma list of svg,dxf,pdf,eps (default: svg)
  -p, --preset <name>     cad | balanced | artistic (default: balanced)
      --colors <n>        palette size K, 2..64
      --line-tol <px>     line fitting tolerance
      --arc-tol <px>      arc fitting tolerance
      --curve-tol <px>    Bezier fitting tolerance
      --corner <deg>      corner interior-angle threshold, 90..160
      --snap <deg>        angle snapping tolerance (0 disables)
      --stacked           stacked layers instead of planar cutout
      --overlay           add colour-coded primitive overlay group to the SVG
      --preview           also write <name>_preview.png (the vector result re-rendered)
      --units <u>         DXF units: none | mm | cm | in (default: none)
      --dxf-scale <f>     DXF drawing units per pixel (default: 1)
  -h, --help              show this help
";

#[derive(Clone)]
struct Opts {
    out: Option<PathBuf>,
    formats: Vec<String>,
    params: Params,
    overlay: bool,
    preview: bool,
    units: DxfUnits,
    dxf_scale: f64,
    runs: usize,
    inputs: Vec<PathBuf>,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        out: None,
        formats: vec!["svg".into()],
        params: Params::balanced(),
        overlay: false,
        preview: false,
        units: DxfUnits::Unitless,
        dxf_scale: 1.0,
        runs: 5,
        inputs: Vec::new(),
    };
    // The preset must be applied first so explicit flags override it.
    if let Some(i) = args.iter().position(|a| a == "-p" || a == "--preset") {
        o.params = match args.get(i + 1).map(String::as_str) {
            Some("cad") => Params::cad(),
            Some("balanced") => Params::balanced(),
            Some("artistic") => Params::artistic(),
            other => return Err(format!("unknown preset {other:?}")),
        };
    }
    let mut it = args.iter().peekable();
    let num = |it: &mut std::iter::Peekable<std::slice::Iter<String>>,
               flag: &str|
     -> Result<f64, String> {
        it.next()
            .ok_or_else(|| format!("{flag} needs a value"))?
            .parse::<f64>()
            .map_err(|e| format!("{flag}: {e}"))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--out" => o.out = Some(PathBuf::from(it.next().ok_or("--out needs a value")?)),
            "-f" | "--formats" => {
                o.formats = it
                    .next()
                    .ok_or("--formats needs a value")?
                    .split(',')
                    .map(|s| s.trim().to_lowercase())
                    .collect();
                for f in &o.formats {
                    if !["svg", "dxf", "pdf", "eps"].contains(&f.as_str()) {
                        return Err(format!("unknown format {f}"));
                    }
                }
            }
            "-p" | "--preset" => {
                it.next();
            }
            "--colors" => o.params.palette_colors = num(&mut it, a)? as usize,
            "--line-tol" => o.params.line_tolerance = num(&mut it, a)?,
            "--arc-tol" => o.params.arc_tolerance = num(&mut it, a)?,
            "--curve-tol" => o.params.curve_tolerance = num(&mut it, a)?,
            "--corner" => o.params.corner_angle = num(&mut it, a)?,
            "--snap" => o.params.snap_angle = num(&mut it, a)?,
            "--stacked" => o.params.topology = TopologyMode::Stacked,
            "--overlay" => o.overlay = true,
            "--preview" => o.preview = true,
            "--units" => {
                o.units = match it.next().map(String::as_str) {
                    Some("none") => DxfUnits::Unitless,
                    Some("mm") => DxfUnits::Millimeters,
                    Some("cm") => DxfUnits::Centimeters,
                    Some("in") => DxfUnits::Inches,
                    other => return Err(format!("unknown units {other:?}")),
                }
            }
            "--dxf-scale" => o.dxf_scale = num(&mut it, a)?,
            "--runs" => o.runs = num(&mut it, a)? as usize,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => o.inputs.push(PathBuf::from(s)),
        }
    }
    Ok(o)
}

fn is_image(p: &Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp")
    )
}

fn expand_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for p in inputs {
        if p.is_dir() {
            let rd = std::fs::read_dir(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let mut v: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| is_image(p))
                .collect();
            v.sort();
            files.extend(v);
        } else if p.is_file() {
            files.push(p.clone());
        } else {
            return Err(format!("{}: no such file or folder", p.display()));
        }
    }
    Ok(files)
}

fn load(path: &Path) -> Result<RasterImage, String> {
    let img = image::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .to_rgba8();
    RasterImage::new(img.width(), img.height(), img.into_raw()).map_err(|e| e.to_string())
}

fn write_outputs(
    doc: &vectorizer_core::VectorDocument,
    base: &Path,
    o: &Opts,
) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::new();
    for f in &o.formats {
        let path = base.with_extension(f);
        let res = match f.as_str() {
            "svg" => std::fs::write(
                &path,
                doc.to_svg(&SvgOptions {
                    primitive_overlay: o.overlay,
                    ..Default::default()
                }),
            ),
            "dxf" => std::fs::write(
                &path,
                doc.to_dxf(&DxfOptions {
                    units: o.units,
                    scale: o.dxf_scale,
                }),
            ),
            "pdf" => std::fs::write(&path, doc.to_pdf()),
            "eps" => std::fs::write(&path, doc.to_eps()),
            _ => unreachable!(),
        };
        res.map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

fn summary(doc: &vectorizer_core::VectorDocument) -> String {
    let s = &doc.stats;
    format!(
        "{:>7.1} ms | {} colours | {} shapes | L {} A {} O {} C {} | nodes {}",
        s.timings.total_ms,
        s.palette_count,
        s.shapes,
        s.lines,
        s.arcs,
        s.circles,
        s.beziers,
        s.nodes
    )
}

fn convert(o: &Opts) -> Result<(), String> {
    let files = expand_inputs(&o.inputs)?;
    if files.is_empty() {
        return Err("no input images".into());
    }
    if let Some(dir) = &o.out {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let t = Instant::now();
    // Whole files run in parallel; each file's pipeline is itself parallel (rayon work-stealing).
    let results: Vec<Result<String, String>> = files
        .par_iter()
        .map(|path| {
            let img = load(path)?;
            let doc = vectorize(&img, &o.params).map_err(|e| format!("{}: {e}", path.display()))?;
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
            let base = match &o.out {
                Some(d) => d.join(stem),
                None => path.with_file_name(stem),
            };
            write_outputs(&doc, &base, o)?;
            if o.preview {
                let name = format!("{stem}_preview.png");
                save_png(&testing::rasterize(&doc, 4), &base.with_file_name(name))?;
            }
            Ok(format!(
                "{:<32} {}",
                path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
                summary(&doc)
            ))
        })
        .collect();
    let mut failed = 0;
    for r in &results {
        match r {
            Ok(line) => println!("{line}"),
            Err(e) => {
                failed += 1;
                eprintln!("error: {e}");
            }
        }
    }
    println!(
        "{} file(s) in {:.0} ms ({} failed)",
        files.len(),
        t.elapsed().as_secs_f64() * 1000.0,
        failed
    );
    if failed > 0 {
        Err(format!("{failed} file(s) failed"))
    } else {
        Ok(())
    }
}

fn save_png(img: &RasterImage, path: &Path) -> Result<(), String> {
    image::RgbaImage::from_raw(img.width, img.height, img.data.clone())
        .ok_or("bad buffer")?
        .save(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn demo(o: &Opts) -> Result<(), String> {
    let dir = o
        .inputs
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("demo-out"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let scenes: Vec<(&str, RasterImage, Params)> = vec![
        (
            "circle_ring",
            testing::scene_circle_ring(512),
            Params::balanced(),
        ),
        ("blueprint", testing::scene_blueprint(512), Params::cad()),
        ("flag", testing::scene_flag(768, 512), Params::balanced()),
        ("sticker", testing::scene_sticker(512), Params::balanced()),
        (
            "rounded_rect",
            testing::scene_rounded_rect(512),
            Params::balanced(),
        ),
        ("blob", testing::scene_blob(512), Params::balanced()),
    ];
    for (name, img, preset) in scenes {
        let mut o2 = o.clone();
        o2.formats = vec!["svg".into(), "dxf".into(), "pdf".into(), "eps".into()];
        o2.params = Params {
            topology: o.params.topology,
            ..preset
        };
        save_png(&img, &dir.join(format!("{name}.png")))?;
        let doc = vectorize(&img, &o2.params).map_err(|e| e.to_string())?;
        write_outputs(&doc, &dir.join(name), &o2)?;
        // Re-render the vector result for visual comparison.
        save_png(
            &testing::rasterize(&doc, 4),
            &dir.join(format!("{name}_vector.png")),
        )?;
        std::fs::write(
            dir.join(format!("{name}_overlay.svg")),
            doc.to_svg(&SvgOptions {
                primitive_overlay: true,
                ..Default::default()
            }),
        )
        .map_err(|e| e.to_string())?;
        println!("{name:<14} {}", summary(&doc));
    }
    println!("demo files written to {}", dir.display());
    Ok(())
}

fn bench(o: &Opts) -> Result<(), String> {
    let path = o.inputs.first().ok_or("bench needs an image")?;
    let img = load(path)?;
    println!(
        "{}x{} px, preset tolerances line {} arc {} curve {}",
        img.width,
        img.height,
        o.params.line_tolerance,
        o.params.arc_tolerance,
        o.params.curve_tolerance
    );
    let _ = vectorize(&img, &o.params).map_err(|e| e.to_string())?; // warm-up
    let mut totals = Vec::new();
    let mut last = None;
    for _ in 0..o.runs.max(1) {
        let t = Instant::now();
        let doc = vectorize(&img, &o.params).map_err(|e| e.to_string())?;
        totals.push(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(doc);
    }
    totals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let doc = last.unwrap();
    let tm = doc.stats.timings;
    println!(
        "total: min {:.1} ms, median {:.1} ms, max {:.1} ms over {} runs",
        totals[0],
        totals[totals.len() / 2],
        totals[totals.len() - 1],
        totals.len()
    );
    println!(
        "stages: prepare {:.1} | quantize {:.1} | topology {:.1} | fit {:.1} ms",
        tm.prepare_ms, tm.quantize_ms, tm.topology_ms, tm.fit_ms
    );
    println!("{}", summary(&doc));
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.is_empty() || args[0] == "gui" || args[0] == "studio" {
        if let Err(e) = server::run_studio_server() {
            eprintln!("Studio server error: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    let cmd = args[0].as_str();
    let result = parse(&args[1..]).and_then(|o| match cmd {
        "convert" => convert(&o),
        "demo" => demo(&o),
        "bench" => bench(&o),
        other => Err(format!("unknown command '{other}' (try --help)")),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
