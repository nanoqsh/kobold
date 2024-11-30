use std::borrow::Cow;
use std::fmt::{self, Debug, Display};
use std::fs;
use std::io;
use std::path::{absolute, Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use leb128::write::unsigned as leb128_write;
use lol_html::{element, html_content::ContentType, rewrite_str, RewriteStrSettings};
use wasmparser::BinaryReaderError;

use crate::log;
use crate::manifest::{manifest, Manifest};
use crate::report::{Error, ErrorExt, Report};

pub fn build() -> Report<()> {
    let Manifest {
        crate_name,
        crate_version,
        mut target,
    } = manifest()?;

    log::building!("{crate_name} v{crate_version}");

    build_cargo_crate()?;

    target.push("wasm32-unknown-unknown");
    target.push("release");
    target.push(&crate_name);
    target.set_extension("wasm");

    if !target.exists() {
        panic!("Couldn't find compiled Wasm: {target:?}");
    }

    let start = Instant::now();

    run_wasm_bindgen(&target)?;

    let dist = Path::new("dist");

    let mut wasm = dist.join(format!("{crate_name}_bg"));
    wasm.set_extension("wasm");

    optimize_wasm(&wasm)?;

    let elapsed = start.elapsed();
    let wasm_path = absolute(&wasm).message("failed to get absolute path")?;
    log::optimized!("wasm `{}` in {elapsed:.2?}", wasm_path.display());

    let mut js = dist.join(&crate_name);
    js.set_extension("js");

    mangle_wasm(&wasm, &js)?;

    let snippets_dir = dist.join("snippets");
    let snippets = read_file_paths(&snippets_dir).message(format!(
        "failed to read {} directory",
        snippets_dir.display(),
    ))?;

    let index = dist.join("index.html");
    let dist = DistPaths {
        snippets: &snippets,
        wasm: &wasm,
        js: &js,
        index: &index,
    };

    make_index_html(Path::new("index.html"), dist)?;

    Ok(())
}

fn build_cargo_crate() -> Report<()> {
    let status = Command::new("cargo")
        .args(["build", "--release", "--target=wasm32-unknown-unknown"])
        .spawn()
        .message("failed to run cargo")?
        .wait()
        .message("failed to build cargo crate")?;

    if !status.success() {
        return Err(Error::message("failed to build cargo crate"));
    }

    Ok(())
}

fn run_wasm_bindgen(target: &Path) -> Report<()> {
    Command::new("wasm-bindgen")
        .arg(target)
        .args(["--out-dir=dist", "--target=web", "--no-typescript"]) //, "--omit-imports"])
        .output()
        .message("failed to run wasm-bindgen")?;

    Ok(())
}

fn optimize_wasm(file: &Path) -> Report<()> {
    Command::new("wasm-opt")
        .arg("-Os")
        .arg(file)
        .arg("-o")
        .arg(file)
        .args(["--enable-simd", "--low-memory-unused"])
        .spawn()
        .message("failed to run wasm-opt")?
        .wait()
        .message("failed to optimize wasm")?;

    Ok(())
}

fn mangle_wasm(wasm: &Path, js: &Path) -> Report<()> {
    let wasm_bytes = fs::read(&wasm).message(format!("failed to read {}", wasm.display()))?;

    let parsed = Wasm::parse(&wasm_bytes)
        .map_err_into_io()
        .message(format!("failed to parse {}", wasm.display()))?;

    // println!(
    //     "Found {} imports amounting to {} bytes",
    //     parsed.imports.len(),
    //     parsed.imports.iter().map(|b| b.name.len()).sum::<usize>()
    // );

    let js_content = fs::read_to_string(&js).message(format!("failed to read {}", js.display()))?;

    let mut remaining = js_content.as_str();

    let mut sym = String::with_capacity(4);
    let mut js_new = String::with_capacity(js_content.len());
    let mut wasm_new = Vec::with_capacity(wasm_bytes.len());
    let mut wasm_imports = Vec::with_capacity(parsed.size);

    wasm_new.extend_from_slice(parsed.head);

    leb128_write(&mut wasm_imports, parsed.imports.len() as u64).expect("write to vec");

    let mut saved = 0;

    let Some(idx) = remaining.find(".wbg") else {
        panic!("Couldn't find the `wbg` imports in the JavaScript input");
    };

    js_new.push_str(&remaining[..idx]);
    js_new.push_str("._");

    remaining = &remaining[idx + 4..];

    for (n, import) in parsed.imports.into_iter().enumerate() {
        let Some(mut idx) = remaining.find(import.name) else {
            panic!(
                "Couldn't find the import {} in the JavaScript input",
                import.name
            );
        };

        if !remaining[..idx].ends_with(".wbg.") {
            continue;
        }

        idx -= 5;

        symbol(n, &mut sym);

        log::info!("renaming wbg.{} to _.{sym}", import.name);

        saved += (2 + import.name.len()) as isize - sym.len() as isize;

        js_new.push_str(&remaining[..idx]);
        js_new.push_str("._.");
        js_new.push_str(&sym);
        wasm_imports.extend_from_slice(b"\x01_");
        wasm_imports.push(sym.len() as u8);
        wasm_imports.extend_from_slice(sym.as_bytes());
        wasm_imports.extend_from_slice(import.ty);

        remaining = &remaining[idx + import.name.len() + 5..];
    }

    js_new.push_str(remaining);

    leb128_write(&mut wasm_new, wasm_imports.len() as u64).expect("write to vec");

    wasm_new.extend_from_slice(&wasm_imports);
    wasm_new.extend_from_slice(parsed.tail);

    log::info!("reduced both .wasm and .js files by {saved} bytes");

    fs::write(&js, js_new).message(format!("failed to write {} file", js.display()))?;
    fs::write(&wasm, wasm_new).message(format!("failed to write {} file", wasm.display()))?;

    Ok(())
}

fn symbol(mut n: usize, buf: &mut String) {
    pub const ALPHABET: [u8; 52] = *b"abcdefghijklmnopqrstuvwxyz\
                                      ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                      ";

    buf.clear();

    loop {
        let byte = ALPHABET[n % 52];

        buf.push(byte as char);

        n /= 52;

        if n == 0 {
            break;
        }

        n -= 1;
    }
}

struct Wasm<'source> {
    /// Byte size of the import section before transforms
    size: usize,
    /// Wasm blob before import section
    head: &'source [u8],
    /// All imports
    imports: Vec<Import<'source>>,
    /// Wasm blob following import section
    tail: &'source [u8],
}

impl Debug for Wasm<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wasm")
            .field("head", &self.head)
            .field("imports", &self.imports)
            .field("tail", &self.tail.len())
            .finish()
    }
}

impl<'source> Wasm<'source> {
    fn parse(source: &'source [u8]) -> Result<Self, BinaryReaderError> {
        use wasmparser::{Parser, Payload};

        let parser = Parser::new(0);

        let mut out = Wasm {
            size: 0,
            head: &[],
            imports: Vec::new(),
            tail: &[],
        };

        for payload in parser.parse_all(source) {
            let section = match payload? {
                Payload::ImportSection(s) => s,
                Payload::ExportSection(s) => {
                    log::info!(
                        "found an export section {:?} {:?}",
                        s.range(),
                        &source[s.range()]
                    );

                    let mut iter = s.into_iter_with_offsets();

                    while let Some(Ok((index, export))) = iter.next() {
                        log::info!("export at {index}: {}", export.name);
                    }
                    continue;
                }
                _ => continue,
            };

            log::info!("found an import section");

            // let Payload::ImportSection(section) = payload? else {
            //     continue;
            // };
            let mut head = &source[..section.range().start - 1];
            let size = section.range().end - section.range().start;
            let wasm = &source[section.range()];
            let tail = &source[section.range().end..];

            while let Some(last) = head.last() {
                // Stop if we are on the import section ID
                if *last == 2 {
                    break;
                }

                head = &head[..head.len() - 1];
            }

            let mut i = 0;
            let mut imports = Vec::with_capacity(section.count() as usize);

            while i < wasm.len() && !wasm[i..].starts_with(b"\x03wbg") {
                i += 1;
            }

            while i < wasm.len() && wasm[i..].starts_with(b"\x03wbg") {
                let len = wasm[i + 4] as usize;

                i += 5;

                let slice = wasm
                    .get(i..i + len)
                    .expect("Couldn't read import name from Wasm");

                let name = std::str::from_utf8(slice).expect("Invalid import name");

                i += len;

                imports.push(Import {
                    name,
                    ty: &wasm[i..i + 2],
                });

                i += 2;
                continue;
            }

            if imports.len() != section.count() as usize {
                panic!("Some imports not found?");
            }

            out.size = size;
            out.head = head;
            out.imports = imports;
            out.tail = tail;
        }

        Ok(out)
    }
}

struct Import<'source> {
    // Found name
    name: &'source str,
    // Unprased Wasm bytes for the type of this import
    ty: &'source [u8],
}

impl Debug for Import<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {:?}", self.name, self.ty)
    }
}

fn read_file_paths(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = vec![];
    let mut to_visit = vec![Cow::Borrowed(path)];
    while let Some(dir) = to_visit.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() {
                paths.push(entry.path());
                continue;
            }

            if file_type.is_dir() {
                to_visit.push(Cow::Owned(entry.path()));
            }
        }
    }

    Ok(paths)
}

struct DistPaths<'path> {
    snippets: &'path [PathBuf],
    wasm: &'path Path,
    js: &'path Path,
    index: &'path Path,
}

fn embed_path(path: &Path) -> impl Display + use<'_> {
    struct Show<'path>(&'path Path);

    impl Display for Show<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "/{}", self.0.display())
        }
    }

    Show(
        path.strip_prefix("dist")
            .expect("the path must have 'dist' prefix"),
    )
}

fn make_index_html(orig_index: &Path, dist: DistPaths<'_>) -> Report<()> {
    fn js_link(path: &Path) -> String {
        format!(
            r#"<link rel="modulepreload" href="{}" crossorigin=anonymous>"#,
            embed_path(path),
        )
    }

    fn wasm_link(path: &Path) -> String {
        format!(
            r#"<link rel="preload" href="{}" crossorigin=anonymous as="fetch" type="application/wasm">"#,
            embed_path(path),
        )
    }

    let js_script = format!(
        "<script type=\"module\">\n\
            import init, * as bindings from '{}';\n\
            window.wasmBindings = bindings;\n\
            await init({{ module_or_path: '{}' }});\n\
        </script>\n",
        embed_path(dist.js),
        embed_path(dist.wasm),
    );

    let html = fs::read_to_string(orig_index)
        .message(format!("failed to read {}", orig_index.display()))?;

    let settings = RewriteStrSettings {
        element_content_handlers: vec![
            element!("head", |el| {
                el.append(&js_link(dist.js), ContentType::Html);
                el.append(&wasm_link(dist.wasm), ContentType::Html);
                for snippet in dist.snippets {
                    el.append(&js_link(snippet), ContentType::Html);
                }

                Ok(())
            }),
            element!("body", |el| {
                el.append(&js_script, ContentType::Html);
                Ok(())
            }),
        ],
        ..RewriteStrSettings::new()
    };

    let html_new = rewrite_str(&html, settings)
        .map_err_into_io()
        .message("failed to rewrite html")?;

    fs::write(dist.index, html_new)
        .message(format!("failed to write {} file", dist.index.display()))?;

    Ok(())
}
