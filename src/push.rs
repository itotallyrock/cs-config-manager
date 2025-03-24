use std::path::PathBuf;

use clap::Args;
use octocrab::OctocrabBuilder;
use tracing::info;

use crate::README_FILE;

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

pub async fn push_config(options: PushOptions) {
    // The text content to upload for the readme
    let readme_content = format!(
        "# Compiled on {}\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    );
    let included_files = crate::get_included_files(&options.cfg_dir, &options.root_file);
    // Number of included files and the readme
    let num_files = included_files.len() + 1;
    // Total bytes of included files and the readme
    let total_bytes = included_files
        .iter()
        .map(|f| f.file_contents.len())
        .sum::<usize>()
        + readme_content.len();

    if options.dry_run {
        info!(
            "skipping uploading {num_files} files ({total_bytes}B) to gist {} due to --dry-run",
            options.gist_id
        );
        return;
    }

    let octocrab = OctocrabBuilder::new()
        .user_access_token(options.github_access_token)
        .build()
        .unwrap();

    // Delete files not found locally
    let current_gist = octocrab.gists().get(&options.gist_id).await.unwrap();
    let deleted_files_names = current_gist.files.keys().filter(|deleted_file| {
        included_files
            .iter()
            .any(|i| i.get_file_name() == deleted_file.as_str())
    });
    let gist = octocrab.gists().update(options.gist_id);
    let gist = deleted_files_names.fold(gist, |gist, deleted_file_name| {
        gist.file(deleted_file_name).delete()
    });

    // Add or update included files on gist
    let gist = included_files
        .into_iter()
        .fold(
            gist.file(README_FILE).with_content(readme_content),
            |gist, included| {
                gist.file(included.get_file_name())
                    .with_content(included.get_formatted_content())
            },
        )
        .send()
        .await
        .unwrap();
    info!(
        "uploaded {num_files} files ({total_bytes}B) to {}",
        gist.html_url
    );
}
