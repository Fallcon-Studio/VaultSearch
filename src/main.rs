use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueHint};
use directories::ProjectDirs;
use html_escape::decode_html_entities;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, SchemaBuilder, TantivyDocument, Value, STORED, TEXT};
use tantivy::snippet::SnippetGenerator;
use tantivy::{doc, Index};

/// Local file search tool (offline, private).
#[derive(Parser, Debug)]
#[command(
    name = "vaultsearch",
    version,
    about = "Vault: local, offline file search",
    author = "You",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize config and index for a root folder
    Init {
        /// Root directory to index (e.g. C:\Users\You\Documents)
        #[arg(long, value_hint = ValueHint::DirPath)]
        root: String,
        /// Recreate the index directory if it already exists
        #[arg(long)]
        force: bool,
    },

    /// Re-scan the filesystem and update the index
    Index,

    /// Search the index for a query string
    Search {
        /// Search query (e.g. "tax report 2023")
        query: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    /// Root directory that will be indexed
    root: String,
    /// Directory where the Tantivy index is stored
    index_dir: String,
    /// Timestamp of last successful indexing run
    #[serde(default)]
    last_indexed: Option<String>,
}

const INDEX_WRITER_HEAP_BYTES: usize = 50_000_000;
const INDEX_PROGRESS_CHUNK: usize = 100;
const TOP_RESULTS: usize = 20;
const MAX_FILE_SIZE_BYTES: u64 = 5_000_000;
const BINARY_SNIFF_BYTES: usize = 4_096;
const TEXT_LIKE_EXTENSIONS: &[&str] = &[
    "txt", "md", "rst", "log", "json", "toml", "yaml", "yml", "ini", "cfg", "rs", "lock", "c",
    "cpp", "h", "hpp", "cs", "java", "py", "go", "rb", "php", "js", "ts", "tsx", "jsx", "html",
    "htm", "css", "sh", "bash", "ps1", "bat", "tex", "csv",
];

// ---- Entry point ----

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { root, force } => {
            cmd_init(&root, force)?;
        }
        Command::Index => {
            cmd_index()?;
        }
        Command::Search { query } => {
            cmd_search(&query)?;
        }
    }

    Ok(())
}

// ---- Commands ----

fn cmd_init(root: &str, force: bool) -> Result<()> {
    // 1) Check the root directory exists.
    let root_path = fs::canonicalize(root)
        .with_context(|| format!("Root path does not exist or is invalid: {root}"))?;
    if !root_path.is_dir() {
        anyhow::bail!("Root path is not a directory: {}", root_path.display());
    }

    // 2) Work out where to put config and index.
    let proj_dirs = get_project_dirs()?;
    let config_path = config_file_path(&proj_dirs)?;
    let index_dir = index_dir_path(&proj_dirs)?;

    // Ensure directories exist.
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let index_already_present = tantivy_index_exists(&index_dir);

    if index_already_present && force {
        println!(
            "Index already exists at {}. Removing and recreating because --force was provided.",
            index_dir.display()
        );
        fs::remove_dir_all(&index_dir).with_context(|| {
            format!(
                "Failed to remove existing index directory: {}",
                index_dir.display()
            )
        })?;
    }

    fs::create_dir_all(&index_dir)
        .with_context(|| format!("Failed to create index directory: {}", index_dir.display()))?;

    // 3) Create or validate the Tantivy index (schema + empty index).
    let index_status = if index_already_present && !force {
        let existing_index = open_index(&index_dir).with_context(|| {
            format!(
                "Failed to open existing index at {}. Re-run with --force to recreate it.",
                index_dir.display()
            )
        })?;

        let existing_schema = existing_index.schema();
        let expected_schema = build_schema();

        if existing_schema != expected_schema {
            anyhow::bail!(
                "Existing index schema does not match expected schema. Re-run with --force to recreate the index."
            );
        }

        "Reused existing Tantivy index."
    } else {
        create_empty_index(&index_dir)?;
        "Created new Tantivy index."
    };

    // 4) Save config file.
    let mut cfg = AppConfig {
        root: root_path.to_string_lossy().to_string(),
        index_dir: index_dir.to_string_lossy().to_string(),
        last_indexed: None,
    };

    write_config(&cfg, &config_path)?;

    println!("Initialized vaultsearch:");
    println!("  Root directory : {}", cfg.root);
    println!("  Index directory: {}", cfg.index_dir);
    println!("  Index status   : {index_status}");
    println!("  Config file    : {}", config_path.display());

    println!("\nStarting initial indexing run...");
    perform_indexing(&mut cfg)?;

    Ok(())
}

fn cmd_index() -> Result<()> {
    let mut cfg = load_config()?;
    perform_indexing(&mut cfg)
}

fn cmd_search(query: &str) -> Result<()> {
    let cfg = load_config()?;
    let index_dir = Path::new(&cfg.index_dir);

    if cfg.last_indexed.is_none() {
        println!(
            "Index has not been built yet for {}. Run `vaultsearch index` to scan your files.",
            cfg.root
        );
        return Ok(());
    }

    if !tantivy_index_exists(index_dir) {
        println!(
            "Index directory missing at {}. Re-run `vaultsearch init` followed by `vaultsearch index`.",
            index_dir.display()
        );
        return Ok(());
    }

    let index = open_index(index_dir)?;
    let schema = index.schema();

    let path_field = schema.get_field("path").expect("path field");
    let contents_field = schema.get_field("contents").expect("contents field");

    let reader = index.reader().context("Failed to create index reader")?;
    let searcher = reader.searcher();

    if searcher.num_docs() == 0 {
        println!(
            "Index is empty. Run `vaultsearch index` to index files under {}.",
            cfg.root
        );
        return Ok(());
    }

    let query_parser = QueryParser::for_index(&index, vec![path_field, contents_field]);

    let tantivy_query = query_parser
        .parse_query(query)
        .with_context(|| format!("Failed to parse query: {query}"))?;

    let mut snippet_generator = SnippetGenerator::create(&searcher, &tantivy_query, contents_field)
        .context("Failed to create snippet generator")?;
    snippet_generator.set_max_num_chars(200);

    let top_docs = searcher
        .search(&tantivy_query, &TopDocs::with_limit(TOP_RESULTS))
        .context("Search failed")?;

    if top_docs.is_empty() {
        println!("No results found for query: {query}");
        return Ok(());
    }

    println!("Results for query: {query}");
    for (rank, (score, doc_address)) in top_docs.into_iter().enumerate() {
        let retrieved_doc: TantivyDocument = searcher
            .doc(doc_address)
            .context("Failed to load document")?;

        let path_value = retrieved_doc
            .get_first(path_field)
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown path>");

        let snippet_html = snippet_generator.snippet_from_doc(&retrieved_doc).to_html();
        let snippet = highlight_snippet(&snippet_html);
        let relative_path = Path::new(path_value)
            .strip_prefix(&cfg.root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path_value.to_string());

        println!("{:>2}. [score: {:.3}] {}", rank + 1, score, relative_path);
        println!("      {snippet}");
        println!();
    }

    Ok(())
}

fn highlight_snippet(snippet_html: &str) -> String {
    let decoded = decode_html_entities(snippet_html);
    let with_bold = decoded.replace("<b>", "\x1b[1m").replace("</b>", "\x1b[0m");
    with_bold
}

fn perform_indexing(cfg: &mut AppConfig) -> Result<()> {
    let root = Path::new(&cfg.root);
    let index_dir = Path::new(&cfg.index_dir);

    if !tantivy_index_exists(index_dir) {
        anyhow::bail!(
            "Index missing at {}. Re-run `vaultsearch init` to recreate it.",
            index_dir.display()
        );
    }

    println!("Indexing...");
    println!("  Root directory : {}", root.display());
    println!("  Index directory: {}", index_dir.display());

    let index = open_index(index_dir)?;
    let schema = index.schema();

    let path_field = schema.get_field("path").expect("path field");
    let contents_field = schema.get_field("contents").expect("contents field");

    // Tantivy index writer: 50 MB heap
    let mut writer = index
        .writer(INDEX_WRITER_HEAP_BYTES)
        .context("Failed to create Tantivy index writer")?;

    // Clear existing documents so the index matches the current filesystem state.
    writer
        .delete_all_documents()
        .context("Failed to clear existing index documents")?;

    let mut indexed_files = 0usize;
    let mut skip_stats = SkipStats::default();

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let path_display = path.display();

        if !is_text_like(path) {
            eprintln!("  [skip] Unsupported extension: {path_display}");
            skip_stats.unsupported_extension += 1;
            continue;
        }

        let metadata = match fs::metadata(path) {
            Ok(meta) => meta,
            Err(e) => {
                eprintln!("  [skip] Failed to read metadata for {path_display}: {e}");
                skip_stats.read_errors += 1;
                continue;
            }
        };

        if metadata.len() > MAX_FILE_SIZE_BYTES {
            eprintln!(
                "  [skip] File exceeds size limit ({} bytes): {path_display}",
                metadata.len()
            );
            skip_stats.too_large += 1;
            continue;
        }

        match is_probably_binary(path) {
            Ok(true) => {
                eprintln!("  [skip] Detected binary content: {path_display}");
                skip_stats.binary += 1;
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("  [skip] Failed to sniff {path_display}: {e}");
                skip_stats.read_errors += 1;
                continue;
            }
        }

        match read_file_streaming(path, metadata.len()) {
            Ok(contents) => {
                let path_str = path.to_string_lossy().to_string();

                let doc = doc!(
                    path_field => path_str,
                    contents_field => contents,
                );

                writer
                    .add_document(doc)
                    .with_context(|| format!("Failed to add document for {}", path.display()))?;

                indexed_files += 1;

                if indexed_files % INDEX_PROGRESS_CHUNK == 0 {
                    println!("  Indexed {indexed_files} files so far...");
                }
            }
            Err(e) => {
                eprintln!("  [skip] Failed to read {}: {e}", path_display);
                skip_stats.read_errors += 1;
            }
        }
    }

    writer.commit().context("Failed to commit index to disk")?;

    cfg.last_indexed = Some(Utc::now().to_rfc3339());
    save_config(cfg)?;

    println!("Indexing complete.");
    println!("  Indexed files : {indexed_files}");
    println!("  Skipped files : {}", skip_stats.total());
    println!(
        "    - Unsupported extension : {}",
        skip_stats.unsupported_extension
    );
    println!("    - Too large             : {}", skip_stats.too_large);
    println!("    - Binary content        : {}", skip_stats.binary);
    println!("    - Read errors           : {}", skip_stats.read_errors);
    println!(
        "  Last indexed  : {}",
        cfg.last_indexed.as_deref().unwrap_or("unknown")
    );

    Ok(())
}

#[derive(Default)]
struct SkipStats {
    unsupported_extension: usize,
    too_large: usize,
    binary: usize,
    read_errors: usize,
}

impl SkipStats {
    fn total(&self) -> usize {
        self.unsupported_extension + self.too_large + self.binary + self.read_errors
    }
}

// ---- Config helpers ----

fn get_project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("com", "vault", "vaultsearch")
        .context("Could not determine platform-specific config directory")
}

fn config_file_path(proj_dirs: &ProjectDirs) -> Result<PathBuf> {
    let mut path = proj_dirs.config_dir().to_path_buf();
    path.push("config.toml");
    Ok(path)
}

fn index_dir_path(proj_dirs: &ProjectDirs) -> Result<PathBuf> {
    let mut path = proj_dirs.data_local_dir().to_path_buf();
    path.push("index");
    Ok(path)
}

fn load_config() -> Result<AppConfig> {
    let proj_dirs = get_project_dirs()?;
    let config_path = config_file_path(&proj_dirs)?;

    let data = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read config file at {}. Did you run `vaultsearch init`?",
            config_path.display()
        )
    })?;

    let cfg: AppConfig = toml::from_str(&data).with_context(|| "Failed to parse config TOML")?;
    Ok(cfg)
}

fn save_config(cfg: &AppConfig) -> Result<()> {
    let proj_dirs = get_project_dirs()?;
    let config_path = config_file_path(&proj_dirs)?;
    write_config(cfg, &config_path)
}

fn write_config(cfg: &AppConfig, config_path: &Path) -> Result<()> {
    let cfg_toml = toml::to_string_pretty(cfg).context("Failed to serialize config to TOML")?;
    fs::write(config_path, cfg_toml)
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;
    Ok(())
}

// ---- Index helpers ----

fn tantivy_index_exists(index_dir: &Path) -> bool {
    index_dir.join("meta.json").exists()
}

fn create_empty_index(index_dir: &Path) -> Result<()> {
    let schema = build_schema();
    let _index =
        Index::create_in_dir(index_dir, schema).context("Failed to create Tantivy index")?;
    Ok(())
}

fn open_index(index_dir: &Path) -> Result<Index> {
    Index::open_in_dir(index_dir).context("Failed to open Tantivy index")
}

fn build_schema() -> Schema {
    let mut schema_builder: SchemaBuilder = Schema::builder();

    // Path: stored so we can print it in results, also tokenized to search by path pieces.
    schema_builder.add_text_field("path", TEXT | STORED);

    // Contents: main text content we will index for full-text search.
    schema_builder.add_text_field("contents", TEXT | STORED);

    schema_builder.build()
}

// ---- File helpers ----

fn is_text_like(path: &Path) -> bool {
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => {
            let ext_lower = ext.to_ascii_lowercase();
            TEXT_LIKE_EXTENSIONS.contains(&ext_lower.as_str())
        }
        None => false,
    }
}

fn is_probably_binary(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file {} for sniffing", path.display()))?;
    let mut buf = [0u8; BINARY_SNIFF_BYTES];
    let bytes_read = file
        .read(&mut buf)
        .with_context(|| format!("Failed to read file {}", path.display()))?;
    let sample = &buf[..bytes_read];

    if sample.iter().any(|&b| b == 0) {
        return Ok(true);
    }

    if std::str::from_utf8(sample).is_err() {
        return Ok(true);
    }

    Ok(false)
}

fn read_file_streaming(path: &Path, size_hint: u64) -> Result<String> {
    let file =
        fs::File::open(path).with_context(|| format!("Failed to open file {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut contents = String::new();
    let mut line = String::new();
    let mut total_bytes: u64 = 0;

    while reader
        .read_line(&mut line)
        .with_context(|| format!("Failed to read from file {}", path.display()))?
        > 0
    {
        total_bytes += line.as_bytes().len() as u64;

        if total_bytes > MAX_FILE_SIZE_BYTES || size_hint > MAX_FILE_SIZE_BYTES {
            anyhow::bail!(
                "File exceeded size limit while reading (limit {} bytes)",
                MAX_FILE_SIZE_BYTES
            );
        }

        contents.push_str(&line);
        line.clear();
    }

    Ok(contents)
}
