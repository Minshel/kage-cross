mod depfile;
mod error;
mod exec;
mod expand;
mod graph;
mod parse;

pub mod ast {
    pub use crate::parse::{BuildFile, DepFormat, Instruction, Statement, SPEC_EXAMPLE};
}

pub use error::{MoldError, Result};
pub use exec::BuildResult;
pub use parse::{parse_file, parse_str};

use std::path::PathBuf;

use crate::graph::{BuildLog, Graph, StatCache};

#[derive(Debug, Clone)]
pub struct Options {
    pub file: PathBuf,
    pub directory: Option<PathBuf>,
    pub jobs: usize,
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub keep_going: bool,
    pub clean: bool,
    pub list_targets: bool,
    pub explain: bool,
    pub compdb: bool,
    pub always_make: bool,
    pub color: Option<bool>,
    pub targets: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            file: PathBuf::from("build.mold"),
            directory: None,
            jobs: default_jobs(),
            dry_run: false,
            verbose: false,
            quiet: false,
            keep_going: false,
            clean: false,
            list_targets: false,
            explain: false,
            compdb: false,
            always_make: false,
            color: None,
            targets: Vec::new(),
        }
    }
}

pub fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

pub fn run(opts: &Options) -> Result<BuildResult> {
    let workdir = match &opts.directory {
        Some(d) => d.clone(),
        None => std::env::current_dir()?,
    };
    let file = if opts.file.is_absolute() {
        opts.file.clone()
    } else {
        workdir.join(&opts.file)
    };
    if !file.is_file() {
        return Err(MoldError::build(format!(
            "missing build file '{}'",
            file.display()
        )));
    }
    let ast = parse::parse_file(&file)?;
    let graph = Graph::from_ast(&ast, workdir.clone())?;

    if opts.list_targets {
        exec::print_targets(&graph);
        return Ok(BuildResult {
            ran: 0,
            failed: 0,
            elapsed_ms: 0,
            nothing_to_do: true,
        });
    }
    if opts.compdb {
        exec::print_compdb(&graph);
        return Ok(BuildResult {
            ran: 0,
            failed: 0,
            elapsed_ms: 0,
            nothing_to_do: true,
        });
    }
    if opts.clean {
        return exec::clean(&graph, opts);
    }

    let mut stat = StatCache::new(workdir.clone());
    let log = BuildLog::load(&workdir);
    let targets = graph.resolve_targets(&opts.targets)?;
    let plan = graph.plan(
        &targets,
        &log,
        &mut stat,
        opts.always_make,
        opts.explain,
    )?;
    exec::execute(&graph, plan, log, stat, opts)
}
