#![deny(clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)]

use std::fs::File;
use std::io::Read;
use std::iter::once;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand};
use compile::CompileOptions;
use futures::future::join_all;
use octocrab::OctocrabBuilder;
use regex::Regex;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{info, Level};
use tracing_subscriber::filter::{FilterExt, LevelFilter, Targets};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{registry, Layer};

mod compile;

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

#[derive(Args, Debug, Clone)]
pub struct PushOptions {
    /// The `./cfg` directory to run against, used to get relative paths from exec calls to include with the files
    #[arg(value_name = "CFG_DIR", value_hint = clap::ValueHint::DirPath)]
    cfg_dir: PathBuf,
    /// The relative path of the root cfg (ie. `autoexec.cfg`) file to run against, following exec calls to concatenate the files
    #[arg(value_name = "AUTOEXEC.CFG", value_hint = clap::ValueHint::FilePath)]
    root_file: PathBuf,
    /// The gist id to publish to
    #[arg(long, required = true)]
    gist_id: String,
    /// The github access token to authenticate using
    #[arg(short = 't', long = "access-token", required = true)]
    github_access_token: String,
    /// Whether or not to actually upload file
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PullOptions {
    /// The `./cfg` directory to run against, used to get relative paths from exec calls to include with the files
    #[arg(value_name = "CFG_DIR", value_hint = clap::ValueHint::DirPath)]
    cfg_dir: PathBuf,
    /// The gist id to publish to
    #[arg(long, required = true)]
    gist_id: String,
    /// The github access token to authenticate using
    #[arg(short = 't', long = "access-token", required = true)]
    github_access_token: String,
    /// Disable creating files if they're not found locally
    #[arg(short = 'u', long = "update-only", action = clap::ArgAction::SetTrue)]
    update_only: bool,
    /// Whether or not to actually write file changes
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
}

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

async fn push_config(options: PushOptions) {
    let octocrab = OctocrabBuilder::new()
        .user_access_token(options.github_access_token)
        .build()
        .unwrap();
    let gist = octocrab.gists().update(options.gist_id);
    let gist = get_included_files(&options.cfg_dir, &options.root_file)
        .iter()
        .fold(
            gist.file("README.md").with_content(format!(
                "# Compiled on {}\n\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            )),
            |gist, included| {
                gist.file(
                    included
                        .relative_file_path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap(),
                )
                .with_content(format!(
                    "// {}\n{}",
                    included.relative_file_path.to_str().unwrap(),
                    included.file_contents
                ))
            },
        );

    if !options.dry_run {
        let gist = gist.send().await.unwrap();
        info!(
            "uploaded {}B to {}",
            gist.files.iter().map(|(_, f)| f.size).sum::<u64>(),
            gist.html_url
        );
    } else {
        info!("skipping uploading due to --dry-run");
    }
}

async fn pull_config(options: PullOptions) {
    join_all(
        OctocrabBuilder::new()
            .user_access_token(options.github_access_token)
            .build()
            .unwrap()
            .gists()
            .get(options.gist_id)
            .await
            .unwrap()
            .files
            .iter()
            .filter(|(file_name, _)| file_name.as_str() != "README.md")
            .map(|(file_name, gist_file)| async {
                let file_contents = gist_file.content.as_ref().unwrap();
                let mut file_lines = file_contents.lines();
                let relative_path = &file_lines.next().unwrap_or(file_name.as_str())[3..];
                let file_contents = file_lines.collect::<Vec<_>>().join("\n");
                let absolute_path = options.cfg_dir.join(relative_path);

                let mut file_write = OpenOptions::new()
                    .write(true)
                    .create(!options.update_only)
                    .open(&absolute_path)
                    .await
                    .unwrap();

                if !options.dry_run {
                    let written_bytes = file_write.write(file_contents.as_bytes()).await.unwrap();

                    info!("wrote {written_bytes}B to {}", absolute_path.display());
                } else {
                    info!(
                        "skipping writing {}B to {} due to --dry-run",
                        file_contents.as_bytes().len(),
                        absolute_path.display()
                    );
                }
            }),
    )
    .await;
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
        CsConfigManagerCommand::Push(options) => push_config(options).await,
        CsConfigManagerCommand::Pull(options) => pull_config(options).await,
    };
}
