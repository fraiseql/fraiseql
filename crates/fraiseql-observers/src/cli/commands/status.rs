//! Status command - display observer runtime status

use crate::error::Result;
use colored::Colorize;
use serde_json::json;

/// Execute status command
pub async fn execute(
    format: crate::cli::OutputFormat,
    _listener: Option<String>,
    _detailed: bool,
) -> Result<()> {
    // In a real implementation, this would:
    // 1. Connect to running observer runtime
    // 2. Fetch listener health status
    // 3. Display current state

    let status_info = json!({
        "listeners": [
            {
                "listener_id": "listener-1",
                "state": "running",
                "is_healthy": true,
                "last_checkpoint": 1000,
                "last_heartbeat": "2026-01-22T12:00:00Z",
                "uptime_seconds": 3600
            },
            {
                "listener_id": "listener-2",
                "state": "running",
                "is_healthy": true,
                "last_checkpoint": 1005,
                "last_heartbeat": "2026-01-22T12:00:01Z",
                "uptime_seconds": 3500
            }
        ],
        "leader": "listener-1",
        "total_listeners": 2,
        "healthy_listeners": 2,
        "timestamp": "2026-01-22T12:00:15Z"
    });

    match format {
        crate::cli::OutputFormat::Json => {
            let json_str = serde_json::to_string_pretty(&status_info)
                .unwrap_or_else(|_| "{}".to_string());
            println!("{json_str}");
        }
        crate::cli::OutputFormat::Text => {
            println!("{}", "Observer Runtime Status".bold().underline());
            println!(
                "\n{}: {} ({})",
                "Leader".cyan(),
                status_info["leader"].as_str().unwrap_or("none").green(),
                format!(
                    "{}/{}",
                    status_info["healthy_listeners"].as_u64().unwrap_or(0),
                    status_info["total_listeners"].as_u64().unwrap_or(0)
                )
                .yellow()
            );

            println!("\n{}", "Listeners:".bold());
            if let Some(listeners) = status_info["listeners"].as_array() {
                for (idx, listener) in listeners.iter().enumerate() {
                    let listener_id = listener["listener_id"].as_str().unwrap_or("unknown");
                    let state = listener["state"].as_str().unwrap_or("unknown");
                    let is_healthy = listener["is_healthy"].as_bool().unwrap_or(false);

                    let status_str = if is_healthy {
                        "✓".green()
                    } else {
                        "✗".red()
                    };

                    println!(
                        "  {}. {} [{}] {}",
                        idx + 1,
                        listener_id.cyan(),
                        state.yellow(),
                        status_str
                    );
                }
            }
        }
    }

    Ok(())
}
