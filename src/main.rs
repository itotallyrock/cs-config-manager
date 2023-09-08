#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]

use octocrab::OctocrabBuilder;
use std::fs::File;
use std::io::{Read, Write};
use std::iter::once;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
struct CsConfigManagerArgs {
    #[command(subcommand)]
    command: CsConfigManagerCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CsConfigManagerCommand {
    Compile(CompileOptions),
    Pull,
    Push(PushOptions),
}


#[derive(Args, Debug, Clone)]
pub struct CompileOptions {
    /// The `./cfg` directory to run against, used to get relative paths from exec calls to concatenate the files
    #[arg()]
    cfg_dir: PathBuf,
    /// The relative path of the root cfg (ie. `autoexec.cfg`) file to run against, following exec calls to concatenate the files
    #[arg()]
    root_file: PathBuf,
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
    /// The gist id to publish to
    #[arg(short = 't', long = "access-token", required = true)]
    github_access_token: String,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum CompileError {
    FileNotFound(PathBuf),
}

fn read_to_string(full_path: &Path) -> String {
    let mut file_contents = String::with_capacity(1024);
    let _ = File::open(full_path).and_then(|mut file| file.read_to_string(&mut file_contents)).unwrap();
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
            .filter_map(|line| exec_regex.captures(line).and_then(|captures| captures.get(1)))
            .flat_map(|exec_file_path| {
                let next_path = exec_file_path.as_str().to_owned() + ".cfg";
                get_included_files(cfg_dir_path, &PathBuf::from(next_path))
            })
        ).collect()
}

fn compile(cfg_dir_path: &Path, path: &Path) -> Result<String, CompileError> {
    let regex = Regex::new(r#"^exec "([^"]+)"|(.+)"#).unwrap();
    let file_contents = read_to_string(path);

    Ok(file_contents
        .lines()
        .map(|line| if let Some(exec_file_path) = regex.captures(line).and_then(|captures| captures.get(1)) {
            compile(cfg_dir_path, &cfg_dir_path.join(exec_file_path.as_str().to_owned() + ".cfg")).unwrap()
        } else {
            line.to_owned()
        })
        .collect::<Vec<String>>()
        .join("\n"))
}

fn compile_and_write(options: CompileOptions) -> PathBuf {
    let root_cfg = options.cfg_dir.join(options.root_file);
    let compiled = compile(&options.cfg_dir, &root_cfg).unwrap();
    let output_path = root_cfg.parent().unwrap().join("compiled.cfg");
    let date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _ = File::create(&output_path).unwrap().write(format!("// Compiled on {date}\n\n{compiled}").as_bytes()).unwrap();

    output_path
}

#[tokio::main]
async fn main() {
    let CsConfigManagerArgs { command } = CsConfigManagerArgs::parse();
    match command {
        CsConfigManagerCommand::Compile(options) => {
            compile_and_write(options);
        },
        CsConfigManagerCommand::Pull => {},
        CsConfigManagerCommand::Push(options) => {
            let octocrab = OctocrabBuilder::new().user_access_token(options.github_access_token).build().unwrap();
            let gist = octocrab.gists().update(options.gist_id);
            get_included_files(&options.cfg_dir, &options.root_file)
                .iter()
                .fold(gist.file("README.md").with_content(format!("# Compiled on {}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))), |gist, included| {
                    gist.file(included.relative_file_path.file_name().unwrap().to_str().unwrap()).with_content(format!("// {}\n{}", included.relative_file_path.to_str().unwrap(), included.file_contents))
                })
                .send()
                .await
                .unwrap();
        },
    }
}
