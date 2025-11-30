use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueHint};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, SchemaBuilder, TantivyDocument, STORED, TEXT};
use tantivy::{doc, Document, Index};

/// Local file search tool (offline, private).
#[derive(Parser, Debug)]
#[command(
    name = "vault",
    version,
    about = "Vault: local, offline file search",
    author = "You"
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
}

const INDEX_WRITER_HEAP_BYTES: usize = 50_000_000;
const INDEX_PROGRESS_CHUNK: usize = 100;
const TOP_RESULTS: usize = 20;
const TEXT_LIKE_EXTENSIONS: &[&str] = &[
    "txt", "md", "rst", "log", "json", "toml", "yaml", "yml", "ini", "cfg", "rs", "lock", "c",
    "cpp", "h", "hpp", "cs", "java", "py", "go", "rb", "php", "js", "ts", "tsx", "jsx", "html",
    "htm", "css", "sh", "bash", "ps1", "bat", "tex", "csv",
];

// ---- Entry point ----

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { root } => {
            cmd_init(&root)?;
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

fn cmd_init(root: &str) -> Result<()> {
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
    fs::create_dir_all(&index_dir)
        .with_context(|| format!("Failed to create index directory: {}", index_dir.display()))?;

    // 3) Create the Tantivy index (schema + empty index).
    create_empty_index(&index_dir)?;

    // 4) Save config file.
    let cfg = AppConfig {
        root: root_path.to_string_lossy().to_string(),
        index_dir: index_dir.to_string_lossy().to_string(),
    };

    let cfg_toml = toml::to_string_pretty(&cfg).context("Failed to serialize config to TOML")?;
    fs::write(&config_path, cfg_toml)
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

    println!("Initialized vaultsearch:");
    println!("  Root directory : {}", cfg.root);
    println!("  Index directory: {}", cfg.index_dir);
    println!("  Config file    : {}", config_path.display());

    Ok(())
}

fn cmd_index() -> Result<()> {
    let cfg = load_config()?;
    let root = Path::new(&cfg.root);
    let index_dir = Path::new(&cfg.index_dir);

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

    let mut indexed_files = 0usize;
    let mut skipped_files = 0usize;

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        // Only index "text-like" files based on extension for now.
        if !is_text_like(path) {
            skipped_files += 1;
            continue;
        }

        match read_file_to_string(path) {
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
                eprintln!("  [skip] Failed to read {}: {e}", path.display());
                skipped_files += 1;
            }
        }
    }

    writer.commit().context("Failed to commit index to disk")?;

    println!("Indexing complete.");
    println!("  Indexed files : {indexed_files}");
    println!("  Skipped files : {skipped_files}");

    Ok(())
}

fn cmd_search(query: &str) -> Result<()> {
    let cfg = load_config()?;
    let index_dir = Path::new(&cfg.index_dir);

    let index = open_index(index_dir)?;
    let schema = index.schema();

    let path_field = schema.get_field("path").expect("path field");
    let contents_field = schema.get_field("contents").expect("contents field");

    let reader = index
        .reader_builder()
        .try_into()
        .context("Failed to create index reader")?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![path_field, contents_field]);

    let tantivy_query = query_parser
        .parse_query(query)
        .with_context(|| format!("Failed to parse query: {query}"))?;

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

        let json = retrieved_doc.to_json(&schema);

        println!("{:>2}. [score: {:.3}] {}", rank + 1, score, json);
    }

    Ok(())
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

// ---- Index helpers ----

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
    schema_builder.add_text_field("contents", TEXT);

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

fn read_file_to_string(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("Failed to open file {}", path.display()))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .with_context(|| format!("Failed to read file {}", path.display()))?;

    // Lossy to avoid panicking on weird encodings
    Ok(String::from_utf8_lossy(&buf).to_string())
}
