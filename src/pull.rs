use crate::README_FILE;
use clap::Args;
use futures::future::join_all;
use octocrab::OctocrabBuilder;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::info;

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

pub async fn pull_config(options: PullOptions) {
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
            .filter(|(file_name, _)| file_name.as_str() != README_FILE)
            .map(|(file_name, gist_file)| async {
                let file_contents = gist_file.content.as_ref().unwrap();
                let mut file_lines = file_contents.lines();
                let relative_path = &file_lines.next().unwrap_or(file_name.as_str())[3..];
                let file_contents = file_lines.collect::<Vec<_>>().join("\n");
                let absolute_path = options.cfg_dir.join(relative_path);
                let path_name = absolute_path.display();

                let mut file_write = OpenOptions::new()
                    .write(true)
                    .create(!options.update_only)
                    .open(&absolute_path)
                    .await
                    .unwrap();

                if options.dry_run {
                    let num_bytes = file_contents.len();
                    info!("skipping writing {num_bytes}B to {path_name} due to --dry-run");
                    return;
                }

                let written_bytes = file_write.write(file_contents.as_bytes()).await.unwrap();
                info!("wrote {written_bytes}B to {path_name}");
            }),
    )
    .await;
}
