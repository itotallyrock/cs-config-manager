#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]

use clap::{Args, Parser, Subcommand};
use octocrab::OctocrabBuilder;
use regex::Regex;
use std::fs::File;
use std::io::{Read, Write};
use std::iter::once;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

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
    /// The github access token to authenticate using
    #[arg(short = 't', long = "access-token", required = true)]
    github_access_token: String,
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

fn compile(cfg_dir_path: &Path, path: &Path) -> String {
    let regex = Regex::new(r#"^exec "([^"]+)"|(.+)"#).unwrap();
    let file_contents = read_to_string(path);

    file_contents
        .lines()
        .map(|line| {
            regex
                .captures(line)
                .and_then(|captures| captures.get(1))
                .map_or_else(
                    || line.to_owned(),
                    |exec_file_path| {
                        compile(
                            cfg_dir_path,
                            &cfg_dir_path.join(exec_file_path.as_str().to_owned() + ".cfg"),
                        )
                    },
                )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn compile_and_write(options: CompileOptions) -> PathBuf {
    let root_cfg = options.cfg_dir.join(options.root_file);
    let compiled = compile(&options.cfg_dir, &root_cfg);
    let output_path = root_cfg.parent().unwrap().join("compiled.cfg");
    let date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _ = File::create(&output_path)
        .unwrap()
        .write(format!("// Compiled on {date}\n\n{compiled}").as_bytes())
        .unwrap();

    output_path
}

#[tokio::main]
async fn main() {
    let CsConfigManagerArgs { command } = CsConfigManagerArgs::parse();
    match command {
        CsConfigManagerCommand::Compile(options) => {
            compile_and_write(options);
        }
        CsConfigManagerCommand::Push(options) => {
            let octocrab = OctocrabBuilder::new()
                .user_access_token(options.github_access_token)
                .build()
                .unwrap();
            let gist = octocrab.gists().update(options.gist_id);
            get_included_files(&options.cfg_dir, &options.root_file)
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
                )
                .send()
                .await
                .unwrap();
        },
        CsConfigManagerCommand::Pull(options) => {
            let octocrab = OctocrabBuilder::new()
                .user_access_token(options.github_access_token)
                .build()
                .unwrap();
            let gist = octocrab.gists().get(options.gist_id).await.unwrap();
            let cfg_files = gist.files.iter().filter(|(file_name, _)| file_name.as_str() != "README.md");
            for (file_name, gist_file) in cfg_files {
                let file_contents = gist_file.content.as_ref().unwrap();
                let mut file_lines = file_contents.lines();
                let relative_path = &file_lines.next().unwrap_or(file_name.as_str())[3..];
                let file_contents = file_lines.collect::<Vec<_>>().join("\n");
                let absolute_path = options.cfg_dir.join(relative_path);

                // Write the file
                OpenOptions::new().write(true).create(true).open(absolute_path).await.unwrap().write(file_contents.as_bytes()).await.unwrap();
            }
        }
    }
}
