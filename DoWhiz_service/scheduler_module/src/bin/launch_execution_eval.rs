use clap::Parser;
use scheduler_module::service::launch_execution::{
    generate_launch_execution_debug_response, LaunchExecutionRequest,
};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "launch_execution_eval")]
struct Args {
    #[arg(long)]
    request_file: PathBuf,
    #[arg(long)]
    output_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let request: LaunchExecutionRequest =
        serde_json::from_str(&fs::read_to_string(&args.request_file)?)?;
    let debug_response = generate_launch_execution_debug_response(request)
        .await
        .map_err(std::io::Error::other)?;

    if let Some(parent) = args.output_file.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(
        &args.output_file,
        serde_json::to_string_pretty(&debug_response)?,
    )?;

    println!("{}", args.output_file.display());
    Ok(())
}
