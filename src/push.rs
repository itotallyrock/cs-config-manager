use clap::Args;
use octocrab::OctocrabBuilder;
use std::path::PathBuf;
use tracing::info;

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
    if options.dry_run {
        info!("skipping uploading due to --dry-run");
    } else {
        let octocrab = OctocrabBuilder::new()
            .user_access_token(options.github_access_token)
            .build()
            .unwrap();
        let gist = octocrab.gists().update(options.gist_id);
        let gist = crate::get_included_files(&options.cfg_dir, &options.root_file)
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
        info!(
            "uploaded {}B to {}",
            gist.files.values().map(|f| f.size).sum::<u64>(),
            gist.html_url
        );
    }
}
