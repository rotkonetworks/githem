use anyhow::Result;
use clap::Parser;
use githem_core::{IngestOptions, Ingester, checkout_branch, is_remote_url, FilterPreset};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "githem")]
#[command(about = "Transform git repositories into LLM-ready text", long_about = None)]
#[command(version, author = "Rotko Networks <hq@rotko.net>")]
struct Cli {
    /// Repository source (local path or git URL, defaults to current directory)
    #[arg(default_value = ".")]
    source: String,

    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Include only files matching pattern (can be specified multiple times)
    #[arg(short, long)]
    include: Vec<String>,

    /// Exclude files matching pattern (in addition to defaults)
    #[arg(short, long)]
    exclude: Vec<String>,

    /// Maximum file size in bytes (default: 1MB)
    #[arg(short = 's', long, default_value = "1048576")]
    max_size: usize,

    /// Branch to checkout (remote repos only)
    #[arg(short, long)]
    branch: Option<String>,

    /// Include untracked files
    #[arg(short = 'u', long)]
    untracked: bool,
    
    /// Path prefix to filter (e.g., "p2p" for monorepo subfolder)
    #[arg(short = 'p', long)]
    path_prefix: Option<String>,

    /// Quiet mode (no header output)
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Filter preset: raw, standard, code-only, minimal
    #[arg(long, value_enum)]
    preset: Option<FilterPresetArg>,

    /// Raw mode - disable all filtering (equivalent to --preset raw)
    #[arg(short = 'r', long, conflicts_with = "preset")]
    raw: bool,

    /// Show filtering statistics without processing
    #[arg(long)]
    stats: bool,
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

    // Determine filter preset
    let filter_preset = if cli.raw {
        Some(FilterPreset::Raw)
    } else if let Some(preset) = cli.preset {
        Some(preset.into())
    } else {
        Some(FilterPreset::Standard) // Default to standard filtering
    };

    let options = IngestOptions {
        include_patterns: cli.include,
        exclude_patterns: cli.exclude,
        max_file_size: cli.max_size,
        include_untracked: cli.untracked,
        branch: cli.branch.clone(),
        path_prefix: cli.path_prefix,
        filter_preset,
        apply_default_filters: false, // We're using explicit presets
    };

    let ingester = if is_remote_url(&cli.source) {
        Ingester::from_url(&cli.source, options)?
    } else {
        let path = PathBuf::from(&cli.source);
        if !path.join(".git").exists() {
            eprintln!("Error: Not a git repository (or any parent up to mount point /)");
            eprintln!("Use 'git init' to create a repository or specify a remote URL");
            std::process::exit(1);
        }

        let ingester = Ingester::from_path(&path, options)?;

        // Handle local branch checkout
        if let Some(branch) = &cli.branch {
            let repo = git2::Repository::open(&path)?;
            checkout_branch(&repo, branch)?;
        }

        ingester
    };

    // Show filtering statistics if requested
    if cli.stats {
        let stats = ingester.get_filter_stats()?;
        
        println!("📊 Filtering Statistics");
        println!("─────────────────────────");
        println!("Total files found: {}", stats.total_files);
        println!("Files to include: {} ({:.1}%)", 
            stats.included_files, 
            stats.inclusion_rate() * 100.0
        );
        println!("Files excluded: {} ({:.1}%)", 
            stats.excluded_files,
            (1.0 - stats.inclusion_rate()) * 100.0
        );
        println!();
        println!("Total size: {:.2} MB", stats.total_size as f64 / 1_048_576.0);
        println!("Included size: {:.2} MB ({:.1}%)", 
            stats.included_size as f64 / 1_048_576.0,
            (1.0 - stats.size_reduction()) * 100.0
        );
        println!("Size reduction: {:.2} MB ({:.1}%)", 
            stats.excluded_size as f64 / 1_048_576.0,
            stats.size_reduction() * 100.0
        );
        
        return Ok(());
    }

    // Setup output
    let mut output: Box<dyn io::Write> = match cli.output {
        Some(path) => Box::new(fs::File::create(path)?),
        None => Box::new(io::stdout()),
    };

    // Write header unless quiet mode
    if !cli.quiet {
        writeln!(output, "# Repository: {}", cli.source)?;
        writeln!(output, "# Generated by githem-cli (rotko.net)")?;
        
        let preset_name = match filter_preset {
            Some(FilterPreset::Raw) => "raw (no filtering)",
            Some(FilterPreset::Standard) => "standard (smart filtering)",
            Some(FilterPreset::CodeOnly) => "code-only",
            Some(FilterPreset::Minimal) => "minimal filtering",
            None => "default",
        };
        writeln!(output, "# Filter preset: {}", preset_name)?;
        
        if !cli.raw && filter_preset != Some(FilterPreset::Raw) {
            writeln!(output, "# Use --raw or --preset raw to include all files")?;
        }
        writeln!(output)?;
    }

    // Show filtering info unless quiet
    if !cli.quiet && filter_preset != Some(FilterPreset::Raw) {
        let stats = ingester.get_filter_stats()?;
        eprintln!("ℹ️  Filtering enabled: {} files → {} files ({:.1}% reduction)", 
            stats.total_files, 
            stats.included_files,
            (1.0 - stats.inclusion_rate()) * 100.0
        );
        eprintln!("ℹ️  Size reduction: {:.2} MB → {:.2} MB ({:.1}% smaller)",
            stats.total_size as f64 / 1_048_576.0,
            stats.included_size as f64 / 1_048_576.0,
            stats.size_reduction() * 100.0
        );
    }

    // Ingest files
    ingester.ingest(&mut output)?;

    Ok(())
}
