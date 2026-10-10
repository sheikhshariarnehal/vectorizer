//! Embedded HTTP Studio server for `vectorizer`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use vectorizer_core::testing;
use vectorizer_core::{
    vectorize, DxfOptions, DxfUnits, Params, RasterImage, SvgOptions, TopologyMode,
};

const STUDIO_HTML: &str = include_str!("studio/index.html");
const STUDIO_CSS: &str = include_str!("studio/style.css");
const STUDIO_JS: &str = include_str!("studio/app.js");

/// Launches the interactive Vectorizer Studio local HTTP server.
pub fn run_studio_server() -> Result<(), Box<dyn std::error::Error>> {
    // Try preferred port 4567, or fall back to OS-assigned ephemeral port
    let listener = TcpListener::bind("127.0.0.1:4567")
        .or_else(|_| TcpListener::bind("127.0.0.1:0"))?;

    let addr = listener.local_addr()?;
    let url = format!("http://127.0.0.1:{}", addr.port());

    println!("======================================================================");
    println!("  VECTORIZER STUDIO (Classical Geometric Vector Engine)");
    println!("  Server running at: {url}");
    println!("  Opening interactive studio in your default browser...");
    println!("  (Press Ctrl+C in this terminal window to stop the server)");
    println!("======================================================================");

    // Open browser on Windows
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn();
    }

    let listener = Arc::new(listener);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || {
                    let _ = handle_client(s);
                });
            }
            Err(e) => eprintln!("Connection error: {e}"),
        }
    }

    Ok(())
}

fn handle_client(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0];
    let full_uri = parts[1];

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    // Read body if Content-Length is present
    let mut body = Vec::new();
    if let Some(cl_str) = headers.get("content-length") {
        if let Ok(cl) = cl_str.parse::<usize>() {
            body.resize(cl, 0);
            reader.read_exact(&mut body)?;
        }
    }

    // Parse path and query params
    let (path, query_str) = match full_uri.split_once('?') {
        Some((p, q)) => (p, q),
        None => (full_uri, ""),
    };
    let query_params = parse_query(query_str);

    // Handle CORS preflight
    if method == "OPTIONS" {
        send_response(&mut stream, 204, "No Content", "text/plain", b"")?;
        return Ok(());
    }

    match (method, path) {
        ("GET", "/") => {
            send_response(&mut stream, 200, "OK", "text/html; charset=utf-8", STUDIO_HTML.as_bytes())?;
        }
        ("GET", "/style.css") => {
            send_response(&mut stream, 200, "OK", "text/css; charset=utf-8", STUDIO_CSS.as_bytes())?;
        }
        ("GET", "/app.js") => {
            send_response(&mut stream, 200, "OK", "application/javascript; charset=utf-8", STUDIO_JS.as_bytes())?;
        }
        ("GET", "/api/demos") => {
            let demos_json = r#"[
  {"id": "circle_ring", "name": "Circles & Rings", "desc": "Pratt Algebraic Circles", "preset": "balanced"},
  {"id": "blueprint", "name": "CAD Blueprint", "desc": "Orthogonal Lines & 90°/45° Snapping", "preset": "cad"},
  {"id": "flag", "name": "Nordic Flag", "desc": "Planar Map & Half-Edge Graph", "preset": "balanced"},
  {"id": "rounded_rect", "name": "Rounded Badge", "desc": "Lines & Tangent Arcs", "preset": "balanced"},
  {"id": "sticker", "name": "Die-Cut Sticker", "desc": "Alpha Transparency & Cutout", "preset": "balanced"},
  {"id": "blob", "name": "Freeform Blob", "desc": "Schneider Cubic Béziers", "preset": "balanced"}
]"#;
            send_response(&mut stream, 200, "OK", "application/json", demos_json.as_bytes())?;
        }
        ("GET", "/api/demo/png") => {
            let id = query_params.get("id").map(|s| s.as_str()).unwrap_or("circle_ring");
            let img = get_demo_image(id);
            let png_bytes = encode_png_memory(&img)?;
            send_response(&mut stream, 200, "OK", "image/png", &png_bytes)?;
        }
        ("POST", "/api/vectorize") => {
            match process_vectorize(&body, &query_params) {
                Ok(json) => {
                    send_response(&mut stream, 200, "OK", "application/json", json.as_bytes())?;
                }
                Err(err) => {
                    let err_json = format!(r#"{{"error": "{}"}}"#, escape_json(&err));
                    send_response(&mut stream, 400, "Bad Request", "application/json", err_json.as_bytes())?;
                }
            }
        }
        ("POST", "/api/export") => {
            let fmt = query_params.get("format").map(|s| s.as_str()).unwrap_or("svg");
            match process_export(&body, &query_params, fmt) {
                Ok((mime, filename, bytes)) => {
                    send_download_response(&mut stream, "OK", mime, &filename, &bytes)?;
                }
                Err(err) => {
                    send_response(&mut stream, 400, "Bad Request", "text/plain", err.as_bytes())?;
                }
            }
        }
        _ => {
            send_response(&mut stream, 404, "Not Found", "text/plain", b"404 Not Found")?;
        }
    }

    Ok(())
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(k.to_lowercase(), v.to_string());
    }
    map
}

fn build_params(query: &HashMap<String, String>) -> Params {
    let mut p = match query.get("preset").map(|s| s.as_str()) {
        Some("cad") => Params::cad(),
        Some("balanced") => Params::balanced(),
        Some("artistic") => Params::artistic(),
        _ => Params::balanced(),
    };

    if let Some(v) = query.get("colors").and_then(|s| s.parse::<usize>().ok()) {
        p.palette_colors = v.clamp(2, 64);
    }
    if let Some(v) = query.get("line_tol").and_then(|s| s.parse::<f64>().ok()) {
        p.line_tolerance = v;
    }
    if let Some(v) = query.get("arc_tol").and_then(|s| s.parse::<f64>().ok()) {
        p.arc_tolerance = v;
    }
    if let Some(v) = query.get("curve_tol").and_then(|s| s.parse::<f64>().ok()) {
        p.curve_tolerance = v;
    }
    if let Some(v) = query.get("corner").and_then(|s| s.parse::<f64>().ok()) {
        p.corner_angle = v;
    }
    if let Some(v) = query.get("snap").and_then(|s| s.parse::<f64>().ok()) {
        p.snap_angle = v;
    }
    if let Some(v) = query.get("stacked") {
        if v == "true" {
            p.topology = TopologyMode::Stacked;
        } else {
            p.topology = TopologyMode::Cutout;
        }
    }
    p
}

fn get_demo_image(id: &str) -> RasterImage {
    match id {
        "circle_ring" => testing::scene_circle_ring(512),
        "blueprint" => testing::scene_blueprint(512),
        "flag" => testing::scene_flag(768, 512),
        "sticker" => testing::scene_sticker(512),
        "rounded_rect" => testing::scene_rounded_rect(512),
        "blob" => testing::scene_blob(512),
        _ => testing::scene_circle_ring(512),
    }
}

fn encode_png_memory(img: &RasterImage) -> Result<Vec<u8>, String> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let rgba = image::RgbaImage::from_raw(img.width, img.height, img.data.clone())
        .ok_or_else(|| "Invalid raster buffer dimensions".to_string())?;
    rgba.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn load_raster_from_bytes(bytes: &[u8]) -> Result<RasterImage, String> {
    let dyn_img = image::load_from_memory(bytes).map_err(|e| format!("Image decode error: {e}"))?;
    let rgba = dyn_img.to_rgba8();
    RasterImage::new(rgba.width(), rgba.height(), rgba.into_raw())
        .map_err(|e| format!("Invalid image: {e}"))
}

fn process_vectorize(body: &[u8], query: &HashMap<String, String>) -> Result<String, String> {
    if body.is_empty() {
        return Err("Empty image body".into());
    }
    let raster = load_raster_from_bytes(body)?;
    let params = build_params(query);

    let doc = vectorize(&raster, &params).map_err(|e| e.to_string())?;

    let svg = doc.to_svg(&SvgOptions::default());
    let overlay_svg = doc.to_svg(&SvgOptions {
        primitive_overlay: true,
        ..Default::default()
    });
    let dxf = doc.to_dxf(&DxfOptions::default());

    let s = &doc.stats;
    let t = &s.timings;

    let json = format!(
        r#"{{
  "success": true,
  "svg": "{}",
  "overlay_svg": "{}",
  "dxf": "{}",
  "stats": {{
    "total_ms": {:.2},
    "prepare_ms": {:.2},
    "quantize_ms": {:.2},
    "topology_ms": {:.2},
    "fit_ms": {:.2},
    "palette_count": {},
    "shapes": {},
    "lines": {},
    "arcs": {},
    "circles": {},
    "beziers": {},
    "nodes": {}
  }}
}}"#,
        escape_json(&svg),
        escape_json(&overlay_svg),
        escape_json(&dxf),
        t.total_ms,
        t.prepare_ms,
        t.quantize_ms,
        t.topology_ms,
        t.fit_ms,
        s.palette_count,
        s.shapes,
        s.lines,
        s.arcs,
        s.circles,
        s.beziers,
        s.nodes
    );

    Ok(json)
}

fn process_export(
    body: &[u8],
    query: &HashMap<String, String>,
    format: &str,
) -> Result<(&'static str, String, Vec<u8>), String> {
    let raster = load_raster_from_bytes(body)?;
    let params = build_params(query);
    let doc = vectorize(&raster, &params).map_err(|e| e.to_string())?;

    match format {
        "svg" => {
            let svg = doc.to_svg(&SvgOptions::default());
            Ok(("image/svg+xml", "vectorized.svg".into(), svg.into_bytes()))
        }
        "overlay" => {
            let ovl = doc.to_svg(&SvgOptions {
                primitive_overlay: true,
                ..Default::default()
            });
            Ok(("image/svg+xml", "primitives_overlay.svg".into(), ovl.into_bytes()))
        }
        "dxf" => {
            let dxf = doc.to_dxf(&DxfOptions {
                units: DxfUnits::Millimeters,
                scale: 1.0,
            });
            Ok(("application/dxf", "vectorized.dxf".into(), dxf.into_bytes()))
        }
        "pdf" => {
            let pdf = doc.to_pdf();
            Ok(("application/pdf", "vectorized.pdf".into(), pdf))
        }
        "eps" => {
            let eps = doc.to_eps();
            Ok(("application/postscript", "vectorized.eps".into(), eps.into_bytes()))
        }
        other => Err(format!("Unknown format '{other}'")),
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 64);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn send_response(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn send_download_response(
    stream: &mut TcpStream,
    reason: &str,
    content_type: &str,
    filename: &str,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    write!(
        stream,
        "HTTP/1.1 200 {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Content-Disposition: attachment; filename=\"{filename}\"\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}
