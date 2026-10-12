//! Read-only, bounded local HTTP serving. Capability-relative filesystem access
//! prevents symlink traversal outside the artifact root, including path races.
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_HEADER: usize = 8192;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const WORKERS: usize = 8;

pub fn serve(directory: &Path, host: &str, port: u16) -> io::Result<()> {
    let root = directory.canonicalize()?;
    if !root.is_dir() {
        return Err(io::Error::other("artifact root must be a directory"));
    }
    let listener = TcpListener::bind((host, port))?;
    let address = listener.local_addr()?;
    println!(
        "{}",
        serde_json::json!({"event":"serving","address":address.to_string(),"url":format!("http://{address}/"),"directory":root,"read_only":true})
    );
    io::stdout().flush()?;
    let (sender, receiver) = mpsc::sync_channel::<TcpStream>(16);
    let receiver = Arc::new(Mutex::new(receiver));
    for _ in 0..WORKERS {
        let receiver = Arc::clone(&receiver);
        let root = root.clone();
        std::thread::spawn(move || loop {
            let stream = match receiver.lock().expect("request queue poisoned").recv() {
                Ok(stream) => stream,
                Err(_) => return,
            };
            let _ = handle(stream, &root);
        });
    }
    for stream in listener.incoming() {
        let stream = stream?;
        if let Err(mpsc::TrySendError::Full(mut stream)) = sender.try_send(stream) {
            stream.set_write_timeout(Some(Duration::from_millis(100)))?;
            let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, root: &Path) -> io::Result<()> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if request.len() >= MAX_HEADER {
            return respond(
                &mut stream,
                431,
                "text/plain",
                b"Request header too large\n",
                false,
            );
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return respond(
                &mut stream,
                408,
                "text/plain",
                b"Request timed out\n",
                false,
            );
        }
        stream.set_read_timeout(Some(remaining))?;
        match stream.read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => request.push(byte[0]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return respond(
                    &mut stream,
                    408,
                    "text/plain",
                    b"Request timed out\n",
                    false,
                );
            }
            Err(error) => return Err(error),
        }
        if request.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let Ok(request) = std::str::from_utf8(&request) else {
        return respond(&mut stream, 400, "text/plain", b"Invalid request\n", false);
    };
    let parts: Vec<_> = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    if parts.len() != 3 || !matches!(parts[2], "HTTP/1.0" | "HTTP/1.1") {
        return respond(
            &mut stream,
            400,
            "text/plain",
            b"Invalid request line\n",
            false,
        );
    }
    let head = parts[0] == "HEAD";
    if !matches!(parts[0], "GET" | "HEAD") {
        return respond(
            &mut stream,
            405,
            "text/plain",
            b"Only GET and HEAD are supported\n",
            false,
        );
    }
    let path = match decode_path(parts[1]) {
        Ok(path) => path,
        Err(()) => {
            return respond(
                &mut stream,
                400,
                "text/plain",
                b"Invalid artifact path\n",
                head,
            )
        }
    };
    if path == Path::new("health") {
        return respond(
            &mut stream,
            200,
            "application/json",
            b"{\"status\":\"ok\",\"read_only\":true}\n",
            head,
        );
    }
    let directory = Dir::open_ambient_dir(root, ambient_authority())?;
    if path.as_os_str().is_empty() {
        return respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            gallery(&directory).as_bytes(),
            head,
        );
    }
    if !directory
        .symlink_metadata(&path)
        .is_ok_and(|metadata| metadata.is_file())
    {
        return respond(
            &mut stream,
            404,
            "text/plain",
            b"Artifact not found\n",
            head,
        );
    }
    let mut file = match directory.open(&path) {
        Ok(file) => file,
        Err(_) => {
            return respond(
                &mut stream,
                404,
                "text/plain",
                b"Artifact not found\n",
                head,
            )
        }
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return respond(
            &mut stream,
            404,
            "text/plain",
            b"Artifact not found\n",
            head,
        );
    }
    if metadata.len() > MAX_FILE {
        return respond(
            &mut stream,
            413,
            "text/plain",
            b"Artifact exceeds 64 MiB serving limit\n",
            head,
        );
    }
    let mut body = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_FILE + 1)
        .read_to_end(&mut body)?;
    if body.len() as u64 > MAX_FILE {
        return respond(
            &mut stream,
            413,
            "text/plain",
            b"Artifact exceeds 64 MiB serving limit\n",
            head,
        );
    }
    let mime = match path.extension().and_then(|value| value.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("csv") => "text/csv; charset=utf-8",
        Some("step" | "stp") => "application/step",
        Some("stl") => "model/stl",
        _ => "application/octet-stream",
    };
    respond(&mut stream, 200, mime, &body, head)
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    body: &[u8],
    head: bool,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let allow = if status == 405 {
        "Allow: GET, HEAD\r\n"
    } else {
        ""
    };
    let header = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n{allow}\r\n", body.len());
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    for mut bytes in [header.as_bytes(), if head { &[] } else { body }] {
        while !bytes.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "response timed out",
                ));
            }
            stream.set_write_timeout(Some(remaining))?;
            let written = stream.write(bytes)?;
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "connection closed",
                ));
            }
            bytes = &bytes[written..];
        }
    }
    Ok(())
}

fn decode_path(target: &str) -> Result<PathBuf, ()> {
    let target = target.split('?').next().ok_or(())?;
    if !target.starts_with('/') {
        return Err(());
    }
    let mut decoded = Vec::new();
    let mut bytes = target.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        let byte = if byte == b'%' {
            let a = (bytes.next().ok_or(())? as char).to_digit(16).ok_or(())?;
            let b = (bytes.next().ok_or(())? as char).to_digit(16).ok_or(())?;
            (a * 16 + b) as u8
        } else {
            byte
        };
        if byte < 32 || byte == 127 || byte == b'\\' || byte == b':' {
            return Err(());
        }
        decoded.push(byte);
    }
    let decoded = String::from_utf8(decoded).map_err(|_| ())?;
    let components: Vec<_> = decoded.split('/').filter(|part| !part.is_empty()).collect();
    if components.iter().any(|part| matches!(*part, "." | "..")) {
        return Err(());
    }
    Ok(components.iter().collect())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn url(path: &str) -> String {
    let mut result = String::from("/");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn gallery(directory: &Dir) -> String {
    fn discover(
        directory: &Dir,
        prefix: &str,
        depth: usize,
        remaining: &mut usize,
        previews: &mut Vec<String>,
        sections: &mut std::collections::BTreeMap<String, Vec<String>>,
    ) {
        if depth > 8 || *remaining == 0 || previews.len() >= 500 {
            return;
        }
        let Ok(entries) = directory.entries() else {
            return;
        };
        let mut entries: Vec<_> = entries.take(*remaining).filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if *remaining == 0 || previews.len() >= 500 {
                break;
            }
            *remaining -= 1;
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let path = format!("{prefix}{name}");
            if kind.is_dir() {
                if let Ok(child) = directory.open_dir(&name) {
                    discover(
                        &child,
                        &format!("{path}/"),
                        depth + 1,
                        remaining,
                        previews,
                        sections,
                    );
                }
            } else if kind.is_file()
                && (name.ends_with(".preview.html") || name.ends_with(".gpu.html"))
            {
                previews.push(path);
            } else if kind.is_file() && name.starts_with("section-") && name.ends_with(".svg") {
                let views = sections
                    .entry(prefix.trim_end_matches('/').to_owned())
                    .or_default();
                if views.len() < cad_build::MAX_SECTION_VIEWS {
                    views.push(path);
                }
            }
        }
    }
    let mut previews = Vec::new();
    let mut sections = std::collections::BTreeMap::new();
    discover(directory, "", 0, &mut 5000, &mut previews, &mut sections);
    let mut models = std::collections::BTreeMap::new();
    for path in previews {
        let stem = path
            .strip_suffix(".preview.html")
            .or_else(|| path.strip_suffix(".gpu.html"))
            .unwrap()
            .to_owned();
        if path.ends_with(".preview.html") || !models.contains_key(&stem) {
            models.insert(stem, path);
        }
    }
    let previews: Vec<_> = models.into_values().collect();
    let mut output = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Headless CAD · Models</title><style>body{margin:0;background:#0d1522;color:#e6edf5;font:16px system-ui;padding:clamp(16px,4vw,40px)}main{max-width:1200px;margin:auto}h1{font-size:36px;margin-bottom:8px}.muted{color:#9aaec4}section{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,280px),1fr));gap:20px;margin-top:32px}article{background:#172438;border:1px solid #2b4059;border-radius:16px;overflow:hidden;padding:20px}img{width:100%;height:210px;object-fit:contain;background:#eff4f9;border-radius:10px}a{color:#91d1ff;text-decoration:none}a:hover{text-decoration:underline}nav{display:flex;gap:16px;flex-wrap:wrap;margin-top:18px}h2{font-size:20px;overflow-wrap:anywhere}</style></head><body><main><p class=\"muted\">RUST KERNEL / READ-ONLY VIEWER</p><h1>Model library</h1><p class=\"muted\">Recipes are the source of truth. Open a generated model to orbit, inspect faces, and download geometry.</p><section>");
    for preview in &previews {
        let stem = preview
            .strip_suffix(".preview.html")
            .or_else(|| preview.strip_suffix(".gpu.html"))
            .expect("preview suffix");
        output.push_str(&format!("<article><a href=\"{}\">", url(preview)));
        let thumbnail = format!("{stem}.preview.svg");
        if directory
            .metadata(&thumbnail)
            .is_ok_and(|metadata| metadata.is_file())
        {
            output.push_str(&format!(
                "<img loading=\"lazy\" src=\"{}\" alt=\"{}\">",
                url(&thumbnail),
                escape(stem)
            ));
        }
        let (folder, name) = stem.rsplit_once('/').unwrap_or(("", stem));
        output.push_str(&format!("<h2>{}</h2></a>", escape(name)));
        if !folder.is_empty() {
            output.push_str(&format!(
                "<small class=\"muted\">{}</small>",
                escape(folder)
            ));
        }
        output.push_str(&format!(
            "<nav><a href=\"{}\">Open viewer ↗</a>",
            url(preview)
        ));
        let gpu = format!("{stem}.gpu.html");
        if &gpu != preview
            && directory
                .metadata(&gpu)
                .is_ok_and(|metadata| metadata.is_file())
        {
            output.push_str(&format!("<a href=\"{}\">GPU viewer ↗</a>", url(&gpu)));
        }
        let stream = format!("{stem}.stream.html");
        if directory
            .metadata(&stream)
            .is_ok_and(|metadata| metadata.is_file())
        {
            output.push_str(&format!("<a href=\"{}\">Stream viewer ↗</a>", url(&stream)));
        }
        for (suffix, label) in [
            ("stream.json", "Stream metadata"),
            ("step", "STEP"),
            ("exchange.json", "Exchange fidelity"),
            ("stl", "STL"),
            ("brep.json", "B-rep JSON"),
            ("assembly.json", "Assembly JSON"),
            ("display.json", "Display JSON"),
            ("bom.json", "BOM JSON"),
            ("bom.csv", "BOM CSV"),
        ] {
            let artifact = format!("{stem}.{suffix}");
            if directory
                .metadata(&artifact)
                .is_ok_and(|metadata| metadata.is_file())
            {
                output.push_str(&format!(
                    "<a download href=\"{}\">{label}</a>",
                    url(&artifact)
                ));
            }
        }
        output.push_str("</nav>");
        if let Some(views) = sections.get(folder) {
            output.push_str(
                "<details style=\"margin-top:18px\"><summary>Recipe sections</summary><nav>",
            );
            for path in views {
                let label = path
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .trim_start_matches("section-")
                    .trim_end_matches(".svg");
                output.push_str(&format!("<a href=\"{}\">{}</a>", url(path), escape(label)));
            }
            output.push_str("</nav></details>");
        }
        output.push_str("</article>");
    }
    if previews.is_empty() {
        output
            .push_str("<p>No model previews yet. Build a recipe into this artifact directory.</p>");
    }
    output.push_str("</section></main></body></html>");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;

    fn request(root: &Path, request: &[u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let root = root.to_owned();
        let worker = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, &root).unwrap();
        });
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(request).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        worker.join().unwrap();
        response
    }

    #[test]
    fn health_files_head_and_method_restriction() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("part.step"), "ISO-10303-21;").unwrap();
        let health = request(
            dir.path(),
            b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(health.starts_with("HTTP/1.1 200"));
        assert!(health.contains("\"read_only\":true"));
        let get = request(
            dir.path(),
            b"GET /part.step HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(get.contains("Content-Type: application/step"));
        assert!(get.contains("Cache-Control: no-store"));
        assert!(get.ends_with("ISO-10303-21;"));
        let head = request(
            dir.path(),
            b"HEAD /part.step HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(head.contains("Content-Length: 13\r\n"));
        assert!(head.ends_with("\r\n\r\n"));
        let post = request(
            dir.path(),
            b"POST /part.step HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(post.starts_with("HTTP/1.1 405"));
        assert!(post.contains("Allow: GET, HEAD"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("part.step")).unwrap(),
            "ISO-10303-21;"
        );
    }

    #[test]
    fn paths_reject_traversal_invalid_encoding_and_platform_prefixes() {
        for path in [
            "/../secret",
            "/%2e%2e/secret",
            "/folder/%2F../secret",
            "/%5csecret",
            "/C:%5csecret",
            "/%00",
            "/%GG",
            "/%FF",
            "http://elsewhere/",
            "/./a",
        ] {
            assert!(decode_path(path).is_err(), "{path}");
        }
        assert_eq!(
            decode_path("/nested/a%20b.step?download=1").unwrap(),
            PathBuf::from("nested/a b.step")
        );
        let dir = tempfile::tempdir().unwrap();
        let response = request(dir.path(), b"GET /%2e%2e/secret HTTP/1.1\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 400"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_file_and_directory_cannot_escape_root() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "not public").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            dir.path().join("file.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).unwrap();
        for path in ["/file.txt", "/escape/secret.txt"] {
            let response = request(
                dir.path(),
                format!("GET {path} HTTP/1.1\r\n\r\n").as_bytes(),
            );
            assert!(response.starts_with("HTTP/1.1 404"));
            assert!(!response.contains("not public"));
        }
    }

    #[test]
    fn gallery_discovers_gpu_only_outputs_without_nonexistent_downloads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("large.gpu.html"), "gpu").unwrap();
        std::fs::write(dir.path().join("large.display.json"), "{}").unwrap();
        std::fs::write(dir.path().join("large.stream.html"), "stream").unwrap();
        std::fs::write(dir.path().join("large.stream.json"), "{}").unwrap();
        let response = request(dir.path(), b"GET / HTTP/1.1\r\n\r\n");
        assert!(response.contains("/large.gpu.html"));
        assert!(response.contains("/large.display.json"));
        assert!(response.contains("/large.stream.html"));
        assert!(response.contains("/large.stream.json"));
        assert!(!response.contains("large.step"));
        assert!(!response.contains("large.preview.svg"));
        std::fs::write(dir.path().join("large.preview.html"), "cpu").unwrap();
        let response = request(dir.path(), b"GET / HTTP/1.1\r\n\r\n");
        assert_eq!(response.matches("<article>").count(), 1);
        assert!(response.contains("GPU viewer"));
    }

    #[test]
    fn gallery_discovers_nested_previews_and_escapes_names() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        for suffix in [
            "preview.html",
            "preview.svg",
            "step",
            "stl",
            "brep.json",
            "exchange.json",
        ] {
            std::fs::write(nested.join(format!("part & test.{suffix}")), "artifact").unwrap();
        }
        let response = request(dir.path(), b"GET / HTTP/1.1\r\n\r\n");
        assert!(response.contains("nested/part &amp; test"));
        assert!(response.contains("/nested/part%20%26%20test.preview.html"));
        assert!(response.contains("/nested/part%20%26%20test.step"));
        assert!(response.contains("/nested/part%20%26%20test.exchange.json"));
        assert!(response.contains("loading=\"lazy\""));
        assert!(response.contains("Recipes are the source of truth"));
    }

    #[test]
    fn oversized_header_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = "GET / HTTP/1.1\r\nX-Large: ";
        let header = format!("{prefix}{}", "a".repeat(MAX_HEADER - prefix.len()));
        let response = request(dir.path(), header.as_bytes());
        assert!(response.starts_with("HTTP/1.1 431"));
    }
}
