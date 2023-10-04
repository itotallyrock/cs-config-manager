use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Args;
use regex::Regex;
use tracing::{debug, info};

#[derive(Args, Debug, Clone)]
pub struct CompileOptions {
    /// The `./cfg` directory to run against, used to get relative paths from exec calls to concatenate the files
    #[arg()]
    cfg_dir: PathBuf,
    /// The relative path of the root cfg (ie. `autoexec.cfg`) file to run against, following exec calls to concatenate the files
    #[arg()]
    root_file: PathBuf,
    /// Whether or not to actually write the file
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
}

fn get_exec_file_path(cfg_dir_path: &Path, exec_file_path: &str) -> PathBuf {
    cfg_dir_path.join(exec_file_path.as_str().to_owned() + ".cfg")
}

fn compile(cfg_dir_path: &Path, path: &Path) -> String {
    debug!("compiling {} in compiled config", path.display());
    let regex = Regex::new(r#"^exec "([^"]+)"|(.+)"#).unwrap();
    let file_contents = crate::read_to_string(path);

    file_contents
        .lines()
        .map(|line| {
            regex
                .captures(line)
                .and_then(|captures| captures.get(1))
                .map_or_else(
                    || line.to_owned(),
                    |exec_file_path| {
                        compile(cfg_dir_path, &get_exec_file_path(cfg_dir_path, exec_file_path.as_str()))
                    },
                )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

pub fn compile_and_write(options: CompileOptions) {
    let root_cfg = options.cfg_dir.join(options.root_file);
    let compiled = compile(&options.cfg_dir, &root_cfg);
    let output_path = root_cfg.parent().unwrap().join("compiled.cfg");
    let date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let compiled = format!("// Compiled on {date}\n\n{compiled}");
    if options.dry_run {
        info!(
            "skipping writing compiled {}B to {} due to --dry-run",
            compiled.as_bytes().len(),
            output_path.display()
        );
    } else {
        let written_bytes = File::create(&output_path)
            .unwrap()
            .write(compiled.as_bytes())
            .unwrap();
        info!("compiled {written_bytes}B to {}", output_path.display());
    }
}
