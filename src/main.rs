#![deny(clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions, clippy::significant_drop_tightening)]

use std::fs::File;
use std::io::Read;
use std::iter::once;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use clap::{Parser, Subcommand};
use compile::CompileOptions;
use pull::PullOptions;
use push::PushOptions;
use regex::Regex;
use tracing::Level;
use tracing_subscriber::filter::{FilterExt, LevelFilter, Targets};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{registry, Layer};

mod compile;
mod pull;
mod push;

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
struct CsConfigManagerArgs {
    #[command(subcommand)]
    command: CsConfigManagerCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CsConfigManagerCommand {
    Compile(CompileOptions),
    Push(PushOptions),
    Pull(PullOptions),
}

pub const README_FILE: &str = "README.md";

fn read_to_string(full_path: &Path) -> String {
    let mut file_contents = String::with_capacity(1024);
    let _ = File::open(full_path)
        .and_then(|mut file| file.read_to_string(&mut file_contents))
        .unwrap();
    file_contents
}

#[derive(Debug)]
struct IncludedFile {
    file_contents: String,
    relative_file_path: PathBuf,
}

impl IncludedFile {
    fn get_formatted_content(&self) -> impl Into<String> {
        format!(
            "// {}\n{}",
            self.relative_file_path.to_str().unwrap(),
            self.file_contents,
        )
    }
}

impl IncludedFile {
    fn get_file_name(&self) -> &str {
        self.relative_file_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    }
}

fn get_included_files(cfg_dir_path: &Path, path: &Path) -> Vec<IncludedFile> {
    static EXEC_REGEX: OnceLock<Regex> = OnceLock::new();
    let exec_regex = EXEC_REGEX.get_or_init(|| Regex::new(r#"^exec "([^"]+)"|(.+)"#).unwrap());

    let full_path = cfg_dir_path.join(path);
    let file_contents = read_to_string(&full_path);

    once(IncludedFile {
        relative_file_path: path.to_path_buf(),
        file_contents: read_to_string(&full_path),
    })
    .chain(
        file_contents
            .lines()
            .filter_map(|line| {
                exec_regex
                    .captures(line)
                    .and_then(|captures| captures.get(1))
            })
            .flat_map(|exec_file_path| {
                let next_path = exec_file_path.as_str().to_owned() + ".cfg";
                get_included_files(cfg_dir_path, &PathBuf::from(next_path))
            }),
    )
    .collect()
}

#[tokio::main]
async fn main() {
    let stdout_subscriber = tracing_subscriber::fmt::layer()
        .without_time()
        .with_target(false)
        .with_filter(
            Targets::new()
                .with_target("cs_config_manager", Level::TRACE)
                .or(LevelFilter::OFF),
        );
    registry().with(stdout_subscriber).init();

    let CsConfigManagerArgs { command } = CsConfigManagerArgs::parse();
    match command {
        CsConfigManagerCommand::Compile(options) => compile::compile_and_write(options),
        CsConfigManagerCommand::Push(options) => push::push_config(options).await,
        CsConfigManagerCommand::Pull(options) => pull::pull_config(options).await,
    }
}
