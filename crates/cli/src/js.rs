use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::codegen::{Codegen, CodegenOptions, CodegenReturn};
use oxc::minifier::{CompressOptions, Minifier, MinifierOptions, MinifierReturn};
use oxc::parser::{Parser, ParserReturn};
use oxc::semantic::{ScopeTree, SemanticBuilder, SemanticBuilderReturn, SymbolTable};
use oxc::span::SourceType;
use oxc::transformer::{TransformOptions, Transformer, TransformerReturn};

use oxc_resolver::{AliasValue, ResolveOptions, Resolver};

fn resolve(source_path: &Path) {
    let mut path = PathBuf::from(source_path);

    path.pop();

    assert!(
        path.is_dir(),
        "{path:?} must be a directory that will be resolved against."
    );
    assert!(path.is_absolute(), "{path:?} must be an absolute path.",);

    println!("path: {path:?}");

    let options = ResolveOptions {
        alias_fields: vec![vec!["browser".into()]],
        alias: vec![("asdf".into(), vec![AliasValue::from("./test.js")])],
        extensions: vec![".js".into(), ".ts".into()],
        extension_alias: vec![(".js".into(), vec![".ts".into(), ".js".into()])],
        // ESM
        condition_names: vec!["node".into(), "import".into()],
        // CJS
        // condition_names: vec!["node".into(), "require".into()],
        ..ResolveOptions::default()
    };

    // match Resolver::new(options).resolve(path, &specifier) {
    //     Err(error) => println!("Error: {error}"),
    //     Ok(resolution) => println!("Resolved: {:?}", resolution.full_path()),
    // }
}

pub fn transform(source: &str, source_path: &Path) -> anyhow::Result<()> {
    // let mut temp = String::new();
    // let mut snippets = PathBuf::from(source_path);

    // snippets.pop();
    // snippets.push("snippets");

    // assert!(snippets.is_dir());

    // let mut dirs: VecDeque<PathBuf> = VecDeque::new();

    // dirs.push_back(snippets);

    // while let Some(dir) = dirs.pop_front() {
    //     for entry in std::fs::read_dir(&dir)? {
    //         let path = entry?.path();

    //         if path.is_dir() {
    //             dirs.push_back(path);
    //             continue;
    //         }

    //         let Some(extension) = path.extension() else {
    //             continue;
    //         };

    //         if extension != "js" {
    //             continue;
    //         }

    //         let mut file = File::open(&path)?;

    //         println!("Bundling: {path:#?}");

    //         file.read_to_string(&mut temp)?;
    //     }
    // }

    // let mut bundled = String::with_capacity(source.len() + temp.len());

    // for mut line in temp.lines() {
    //     const EXPORT: &str = "export ";
    //     if line.starts_with(EXPORT) {
    //         line = &line[EXPORT.len()..];
    //     }
    //     bundled.push_str(line);
    // }

    // drop(temp);

    // bundled.push_str(source);

    // Memory arena where AST nodes are allocated.
    let allocator = Allocator::default();

    let ParserReturn {
        mut program,
        errors,
        panicked,
        ..
    } = Parser::new(&allocator, &source, SourceType::cjs()).parse();

    assert!(!panicked);
    assert!(errors.is_empty());

    let SemanticBuilderReturn { semantic, errors } = SemanticBuilder::new().build(&program);

    let (symbols, scopes) = semantic.into_symbol_table_and_scope_tree();

    assert!(errors.is_empty());

    let options = TransformOptions {
        ..Default::default()
    };

    let TransformerReturn {
        errors,
        symbols,
        scopes,
    } = Transformer::new(&allocator, source_path, options).build_with_symbols_and_scopes(
        symbols,
        scopes,
        &mut program,
    );

    assert!(errors.is_empty());

    let options = MinifierOptions {
        mangle: true,
        compress: CompressOptions::all_true(),
    };

    let MinifierReturn { mangler } = Minifier::new(options).build(&allocator, &mut program);

    let options = CodegenOptions {
        minify: true,
        single_quote: true,
        comments: false,
        annotation_comments: false,
        ..CodegenOptions::default()
    };

    let CodegenReturn { code, .. } = Codegen::new()
        .with_options(options)
        .with_mangler(mangler)
        .build(&program);

    panic!("{code}");

    Ok(())
}
