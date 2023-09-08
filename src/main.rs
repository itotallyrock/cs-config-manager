use std::fs::File;
use std::io::Read;
use regex::Regex;
use std::path::{Path, PathBuf};
use clap::{Args, Parser, Subcommand};

// use octocrab::OctocrabBuilder;
// use futures::{future, JoinAll};
// use regex::Regex;
// use itertools::Either;
#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
struct CsConfigManagerArgs {
    #[command(subcommand)]
    command: CsConfigManagerCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CsConfigManagerCommand {
    Sync,
    Compile(CompileOptions),
    Publish,
}

#[derive(Args, Debug, Clone)]
pub struct CompileOptions {
    /// The `./cfg` directory to run against, used to get relative paths from exec calls to concatenate the files
    #[arg()]
    cfg_dir: String,
    /// The relative path of the root cfg (ie. `autoexec.cfg`) file to run against, following exec calls to concatenate the files
    #[arg()]
    root_file: String,
}

// #[derive(Copy, Clone, Eq, PartialEq, Debug)]
// pub enum ConflictResolutionStrategy {
//     UseRemote,
//     UseLocal,
//     Abort,
// }
//
// pub struct SyncOptions {
//     files: Option<Vec<String>>,
//     /// How to resolve a conflict or difference between synced files
//     #[arg(short = "r", long = "conflict-resolution", value_enum, default_value_t = ConflictResolutionStrategy::Abort)]
//     conflict_resolution: ConflictResolutionStrategy,
// }

fn publish() {
    todo!("Upload files to github gist")
}

fn sync() {
    todo!("Download files from github gist")
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum CompileError {
    FileNotFound(PathBuf),
}

fn compile(cfg_dir_path: &Path, path: &Path) -> Result<String, CompileError> {
    let regex = Regex::new(r#"^exec "([^"]+)"|(.+)"#).unwrap();
    let file_contents = {
        let mut buffer = String::with_capacity(1024);
        let _ = File::open(path).and_then(|mut file| file.read_to_string(&mut buffer)).map_err(|_| CompileError::FileNotFound(path.to_owned()))?;
        buffer
    };

    let path = path.ancestors().nth(1).unwrap();
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

fn main() {
    // OctocrabBuilder::new().user_access_token(github_access_token).build().unwrap();
    // println!("Hello, world!");
    let CsConfigManagerArgs { command } = CsConfigManagerArgs::parse();
    match command {
        CsConfigManagerCommand::Sync => {}
        CsConfigManagerCommand::Compile(options) => println!("{}", compile(Path::new(options.cfg_dir.clone().as_str()), PathBuf::from(options.cfg_dir).join(Path::new(options.root_file.as_str())).as_path().clone()).unwrap()),
        CsConfigManagerCommand::Publish => {}
    }
}
