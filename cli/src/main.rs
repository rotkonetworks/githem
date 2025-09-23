use anyhow::Result;
use clap::Parser;
use githem_core::{
    checkout_branch, is_remote_url, parse_github_url, CacheManager, FilterPreset, GitHubUrlType,
    IngestOptions, Ingester,
};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "githem")]
#[command(about = "Transform git repositories into LLM-ready text", long_about = None)]
#[command(version, author = "Rotko Networks <hq@rotko.net>")]
struct Cli {
    /// Repository source
    #[arg(default_value = ".")]
    source: String,

    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Include only files matching pattern (use trailing / for directories)
    #[arg(short, long)]
    include: Vec<String>,

    /// Exclude files matching pattern
    #[arg(short, long)]
    exclude: Vec<String>,

    /// Maximum file size in bytes
    #[arg(short = 's', long, default_value = "1048576")]
    max_size: usize,

    /// Branch to checkout
    #[arg(short, long)]
    branch: Option<String>,

    /// Include untracked files
    #[arg(short = 'u', long)]
    untracked: bool,

    /// Path prefix to filter
    #[arg(short = 'p', long)]
    path_prefix: Option<String>,

    /// Quiet mode
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Filter preset: raw, standard, code-only, minimal
    #[arg(long, value_enum)]
    preset: Option<FilterPresetArg>,

    /// Raw mode - disable all filtering
    #[arg(short = 'r', long, conflicts_with = "preset")]
    raw: bool,

    /// Show filtering statistics
    #[arg(long)]
    stats: bool,

    /// Disable cache
    #[arg(long)]
    no_cache: bool,

    /// Clear all cache
    #[arg(long)]
    clear_cache: bool,

    /// Show cache statistics
    #[arg(long)]
    cache_stats: bool,

    /// Force refresh (ignore cache)
    #[arg(long, short = 'f')]
    force: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum FilterPresetArg {
    Raw,
    Standard,
    CodeOnly,
    Minimal,
}

impl From<FilterPresetArg> for FilterPreset {
    fn from(arg: FilterPresetArg) -> Self {
        match arg {
            FilterPresetArg::Raw => FilterPreset::Raw,
            FilterPresetArg::Standard => FilterPreset::Standard,
            FilterPresetArg::CodeOnly => FilterPreset::CodeOnly,
            FilterPresetArg::Minimal => FilterPreset::Minimal,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle cache management commands
    if cli.cache_stats {
        let stats = CacheManager::get_stats()?;
        println!("📊 Cache Statistics");
        println!("─────────────────────");
        println!("Location: {}", stats.cache_dir.display());
        println!("Entries: {}", stats.total_entries);
        println!(
            "Size: {:.2} MB / {:.2} MB",
            stats.total_size as f64 / 1_048_576.0,
            stats.max_size as f64 / 1_048_576.0
        );
        println!("Expired: {}", stats.expired_entries);
        return Ok(());
    }

    if cli.clear_cache {
        CacheManager::clear_cache()?;
        println!("✓ Cache cleared successfully");
        if cli.source == "." && !PathBuf::from(".").join(".git").exists() {
            return Ok(());
        }
    }

    let parsed_result = parse_source(&cli.source);

    match parsed_result {
        SourceType::Local(path) => handle_local_repo(path, cli),
        SourceType::GitUrl(url) => handle_git_url(url, cli),
        SourceType::GitHub {
            owner,
            repo,
            branch,
            path,
            url_type,
        } => match url_type {
            GitHubUrlType::Compare => handle_compare(&owner, &repo, branch.as_deref(), cli),
            _ => handle_github_repo(owner, repo, branch, path, cli),
        },
    }
}

enum SourceType {
    Local(String),
    GitUrl(String),
    GitHub {
        owner: String,
        repo: String,
        branch: Option<String>,
        path: Option<String>,
        url_type: GitHubUrlType,
    },
}

fn parse_source(source: &str) -> SourceType {
    if let Some(parsed) = parse_github_url(source) {
        return SourceType::GitHub {
            owner: parsed.owner,
            repo: parsed.repo,
            branch: parsed.branch,
            path: parsed.path,
            url_type: parsed.url_type,
        };
    }

    if !source.contains("://") && source.matches('/').count() == 1 {
        let parts: Vec<&str> = source.split('/').collect();
        if parts.len() == 2 {
            return SourceType::GitHub {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                branch: None,
                path: None,
                url_type: GitHubUrlType::Repository,
            };
        }
    }

    if !source.contains("://") && source.contains("/compare/") {
        let parts: Vec<&str> = source.splitn(4, '/').collect();
        if parts.len() == 4 && parts[2] == "compare" {
            return SourceType::GitHub {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                branch: Some(parts[3].to_string()),
                path: None,
                url_type: GitHubUrlType::Compare,
            };
        }
    }

    if is_remote_url(source) {
        return SourceType::GitUrl(source.to_string());
    }

    SourceType::Local(source.to_string())
}

fn handle_compare(owner: &str, repo: &str, compare_spec: Option<&str>, cli: Cli) -> Result<()> {
    let compare_spec = compare_spec.ok_or_else(|| anyhow::anyhow!("Compare spec is required"))?;

    let (base, head) = parse_compare_spec(compare_spec)
        .ok_or_else(|| anyhow::anyhow!("Invalid compare format"))?;

    let url = format!("https://github.com/{}/{}", owner, repo);

    let options = create_ingest_options(&cli);
    let ingester = Ingester::from_url(&url, options)?;

    let diff_content = ingester.generate_diff(&base, &head)?;

    let mut output: Box<dyn io::Write> = match cli.output {
        Some(path) => Box::new(fs::File::create(path)?),
        None => Box::new(io::stdout()),
    };

    write!(output, "{}", diff_content)?;

    Ok(())
}

fn handle_github_repo(
    owner: String,
    repo: String,
    branch: Option<String>,
    path: Option<String>,
    cli: Cli,
) -> Result<()> {
    let url = format!("https://github.com/{}/{}", owner, repo);

    let mut options = create_ingest_options(&cli);
    options.branch = branch.or(cli.branch.clone());
    options.path_prefix = path.or(cli.path_prefix.clone());

    process_repository(&url, options, cli)
}

fn handle_git_url(url: String, cli: Cli) -> Result<()> {
    let options = create_ingest_options(&cli);
    process_repository(&url, options, cli)
}

fn handle_local_repo(path: String, cli: Cli) -> Result<()> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.join(".git").exists() {
        eprintln!("Error: Not a git repository");
        std::process::exit(1);
    }

    let options = create_ingest_options(&cli);
    let ingester = Ingester::from_path(&path_buf, options)?;

    if let Some(branch) = &cli.branch {
        let repo = git2::Repository::open(&path_buf)?;
        checkout_branch(&repo, branch)?;
    }

    process_with_ingester(ingester, cli)
}

fn create_ingest_options(cli: &Cli) -> IngestOptions {
    let filter_preset = if cli.raw {
        Some(FilterPreset::Raw)
    } else if let Some(preset) = &cli.preset {
        Some(preset.clone().into())
    } else {
        Some(FilterPreset::Standard)
    };

    IngestOptions {
        include_patterns: cli.include.clone(),
        exclude_patterns: cli.exclude.clone(),
        max_file_size: cli.max_size,
        include_untracked: cli.untracked,
        branch: cli.branch.clone(),
        path_prefix: cli.path_prefix.clone(),
        filter_preset,
        apply_default_filters: false,
    }
}

fn process_repository(url: &str, options: IngestOptions, cli: Cli) -> Result<()> {
    let ingester = if cli.no_cache || cli.force {
        Ingester::from_url(url, options)?
    } else {
        Ingester::from_url_cached(url, options)?
    };

    process_with_ingester(ingester, cli)
}

fn process_with_ingester(mut ingester: Ingester, cli: Cli) -> Result<()> {
    if cli.stats {
        show_stats(&ingester)?;
        return Ok(());
    }

    let mut output: Box<dyn io::Write> = match cli.output {
        Some(ref path) => Box::new(fs::File::create(path)?),
        None => Box::new(io::stdout()),
    };

    if !cli.quiet {
        write_header(&mut output, &cli)?;
    }

    if !cli.quiet && !matches!(ingester.get_filter_preset(), Some(FilterPreset::Raw)) {
        show_filtering_info(&ingester)?;
    }

    // Use cached ingestion if enabled
    if !cli.no_cache && !cli.force && ingester.cache_key.is_some() {
        ingester.ingest_cached(&mut output)?;
    } else {
        ingester.ingest(&mut output)?;
    }

    Ok(())
}

fn parse_compare_spec(spec: &str) -> Option<(String, String)> {
    if let Some((base, head)) = spec.split_once("...") {
        Some((base.to_string(), head.to_string()))
    } else if let Some((base, head)) = spec.split_once("..") {
        Some((base.to_string(), head.to_string()))
    } else {
        None
    }
}

fn write_header(output: &mut dyn io::Write, cli: &Cli) -> Result<()> {
    writeln!(output, "# Repository: {}", cli.source)?;
    writeln!(output, "# Generated by githem-cli (rotko.net)")?;

    let preset_name = if cli.raw {
        "raw (no filtering)"
    } else if let Some(preset) = &cli.preset {
        match preset {
            FilterPresetArg::Raw => "raw (no filtering)",
            FilterPresetArg::Standard => "standard (smart filtering)",
            FilterPresetArg::CodeOnly => "code-only",
            FilterPresetArg::Minimal => "minimal filtering",
        }
    } else {
        "standard (smart filtering)"
    };

    writeln!(output, "# Filter preset: {}", preset_name)?;

    if !cli.no_cache && !cli.force {
        writeln!(output, "# Cache: enabled (use --no-cache to disable)")?;
    }

    writeln!(output)?;

    Ok(())
}

fn show_stats(ingester: &Ingester) -> Result<()> {
    let stats = ingester.get_filter_stats()?;

    println!("📊 Filtering Statistics");
    println!("─────────────────────────");
    println!("Total files found: {}", stats.total_files);
    println!(
        "Files to include: {} ({:.1}%)",
        stats.included_files,
        stats.inclusion_rate() * 100.0
    );
    println!(
        "Files excluded: {} ({:.1}%)",
        stats.excluded_files,
        (1.0 - stats.inclusion_rate()) * 100.0
    );
    println!();
    println!(
        "Total size: {:.2} MB",
        stats.total_size as f64 / 1_048_576.0
    );
    println!(
        "Included size: {:.2} MB ({:.1}%)",
        stats.included_size as f64 / 1_048_576.0,
        (1.0 - stats.size_reduction()) * 100.0
    );
    println!(
        "Size reduction: {:.2} MB ({:.1}%)",
        stats.excluded_size as f64 / 1_048_576.0,
        stats.size_reduction() * 100.0
    );

    Ok(())
}

fn show_filtering_info(ingester: &Ingester) -> Result<()> {
    let stats = ingester.get_filter_stats()?;
    eprintln!(
        "ℹ️  Filtering: {} → {} files ({:.1}% reduction)",
        stats.total_files,
        stats.included_files,
        (1.0 - stats.inclusion_rate()) * 100.0
    );
    eprintln!(
        "ℹ️  Size: {:.2} MB → {:.2} MB ({:.1}% smaller)",
        stats.total_size as f64 / 1_048_576.0,
        stats.included_size as f64 / 1_048_576.0,
        stats.size_reduction() * 100.0
    );

    Ok(())
}
