use std::path::PathBuf;
use std::process::ExitCode;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::Parser;
use kage::{default_jobs, Options};

fn clap_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .usage(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default())
}

#[derive(Parser, Debug)]
#[command(
    name = "kage",
    version,
    about = "A fast, ninja-inspired build system",
    long_about = "kage reads a build.kage file, builds a dependency graph, and \
runs compile/link instructions in parallel. Incremental rebuilds use mtimes, \
command-line hashing, and GCC-style depfiles — the no-op path is the fast path.",
    styles = clap_styles(),
    after_help = "EXAMPLES:\n  \
        kage\n  \
        kage -j8 example_project\n  \
        kage --explain\n  \
        kage --clean\n  \
        kage -C path/to/project -f build.kage"
)]
struct Cli {
    #[arg(short, long, default_value = "build.kage", value_name = "FILE")]
    file: PathBuf,

    #[arg(short = 'C', long, value_name = "DIR")]
    directory: Option<PathBuf>,

    #[arg(short, long, value_name = "N")]
    jobs: Option<usize>,

    #[arg(short = 'n', long)]
    dry_run: bool,

    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long)]
    quiet: bool,

    #[arg(short = 'k', long)]
    keep_going: bool,

    #[arg(long)]
    clean: bool,

    #[arg(long)]
    list_targets: bool,

    #[arg(long)]
    explain: bool,

    #[arg(long)]
    compdb: bool,

    #[arg(short = 'B', long)]
    always_make: bool,

    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: String,

    #[arg(value_name = "TARGET")]
    targets: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let color = match cli.color.as_str() {
        "always" | "on" | "yes" => Some(true),
        "never" | "off" | "no" => Some(false),
        _ => None,
    };
    let opts = Options {
        file: cli.file,
        directory: cli.directory,
        jobs: cli.jobs.unwrap_or_else(default_jobs).max(1),
        dry_run: cli.dry_run,
        verbose: cli.verbose,
        quiet: cli.quiet,
        keep_going: cli.keep_going,
        clean: cli.clean,
        list_targets: cli.list_targets,
        explain: cli.explain,
        compdb: cli.compdb,
        always_make: cli.always_make,
        color,
        targets: cli.targets,
    };
    match kage::run(&opts) {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
