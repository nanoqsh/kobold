use std::fmt::{self, Debug};
use std::fs;
use std::path::{absolute, Path};
use std::process::Command;

use leb128::write::unsigned as leb128_write;
use wasmparser::BinaryReaderError;

use crate::log;
use crate::manifest::manifest;
use crate::report::{Report, ReportExt};

pub fn build() -> Result<(), Report> {
    let manifest = manifest()?;

    let mut target = manifest.target.clone();

    target.push("wasm32-unknown-unknown");
    target.push("release");
    target.push(&manifest.crate_name);
    target.set_extension("wasm");

    if !manifest.target.exists() {
        panic!("Couldn't find compiled Wasm: {target:?}");
    }

    let start = std::time::Instant::now();

    Command::new("wasm-bindgen")
        .arg(&target)
        .args(["--out-dir=dist", "--target=web", "--no-typescript"]) //, "--omit-imports"])
        .output()
        .with_message("failed to run wasm-bindgen")?;

    let dist_dir = Path::new("dist");

    let mut wasm = dist_dir.join(format!("{}_bg", &manifest.crate_name));
    wasm.set_extension("wasm");

    optimize_wasm(&wasm)?;

    let elapsed = start.elapsed();
    let wasm_path = absolute(&wasm).with_message("failed to get absolute path")?;
    log::optimized!("wasm `{}` in {elapsed:.2?}", wasm_path.display());

    // return Ok(());

    let wasm_bytes = fs::read(&wasm).with_message(format!("failed to read {}", wasm.display()))?;

    let parsed =
        Wasm::parse(&wasm_bytes).with_message(format!("failed to parse {}", wasm.display()))?;

    // println!(
    //     "Found {} imports amounting to {} bytes",
    //     parsed.imports.len(),
    //     parsed.imports.iter().map(|b| b.name.len()).sum::<usize>()
    // );

    let mut input = dist_dir.join(&manifest.crate_name);
    input.set_extension("js");

    let js =
        fs::read_to_string(&input).with_message(format!("failed to read {}", input.display()))?;

    // js::transform(&js, &input);
    // panic!();

    let mut remaining = js.as_str();

    let mut sym = String::with_capacity(4);
    let mut js_new = String::with_capacity(js.len());
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

    log::reduced!("both .wasm and .js files by {saved} bytes");

    fs::write(&input, &js_new).with_message(format!("failed to write {} file", input.display()))?;
    fs::write(&wasm, &wasm_new).with_message(format!("failed to write {} file", wasm.display()))?;

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

fn optimize_wasm(file: &Path) -> Result<(), Report> {
    Command::new("wasm-opt")
        .arg("-Os")
        .arg(file)
        .arg("-o")
        .arg(file)
        .args(["--enable-simd", "--low-memory-unused"])
        .spawn()
        .with_message("failed to run wasm-opt")?
        .wait()
        .with_message("failed to optimize wasm")?;

    Ok(())
}
