//! Logging infrastructure for the Balatro Mod Manager.
//!
//! This module sets up a dual-output logging system that writes to both
//! stdout (with ANSI colors) and a rotating log file.
//!
//! # Features
//!
//! - Colorized console output with level-based highlighting
//! - Automatic log file rotation (keeps last 10 logs)
//! - Timestamped log entries
//! - Thread-safe initialization guard
//!
//! # Log Location
//!
//! Logs are stored in `<config_dir>/Balatro/logs/bmm_<timestamp>.log`
//!
//! # Example
//!
//! ```ignore
//! use bmm_lib::logging::init_logger;
//!
//! init_logger()?;
//! log::info!("Application started");
//! ```

use crate::errors::AppError;
use chrono::Local;
use log::LevelFilter;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static LOGGER_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init_logger() -> Result<(), AppError> {
    // Only initialize once
    if LOGGER_INITIALIZED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    // Create log directory in config dir
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::DirNotFound(PathBuf::from("config directory")))?;
    let log_dir = config_dir.join("Balatro").join("logs");

    fs::create_dir_all(&log_dir).map_err(|e| AppError::DirCreate {
        path: log_dir.clone(),
        source: e.to_string(),
    })?;

    // Create a unique log file with timestamp
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let log_file = log_dir.join(format!("bmm_{timestamp}.log"));

    // Clean up old log files
    cleanup_old_logs(&log_dir)?;

    // Open log file
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true) // removed .write(true)
        .open(&log_file)
        .map_err(|e| AppError::FileWrite {
            path: log_file.clone(),
            source: e.to_string(),
        })?;

    // Create a combined writer for both file and stdout
    let file_writer = CustomWriter { file };

    // Initialize env_logger with our custom writer
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] {:<5} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .filter_module("discord_rich_presence", LevelFilter::Warn)
        .filter(None, LevelFilter::Debug) // Capture all logs
        .write_style(env_logger::WriteStyle::Never)
        .target(env_logger::Target::Pipe(Box::new(file_writer)))
        .init();

    // Log some initial messages to test
    log::info!("Logging system initialized at {}", Local::now());
    log::info!("Log file: {}", log_file.display());
    log::debug!("Debug logging is enabled");

    Ok(())
}

// Custom writer that writes to both a file and stdout
struct CustomWriter {
    file: fs::File,
}

// ANSI color codes
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const MAGENTA: &str = "\x1b[35m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Add ANSI color codes to a log line based on the log level
fn colorize_log_line(line: &str) -> String {
    // Find the log level in the line and colorize accordingly
    // Format: [timestamp] LEVEL [target] message
    if let Some(level_start) = line.find("] ") {
        let after_timestamp = &line[level_start + 2..];
        let (level_color, level_end) = if after_timestamp.starts_with("ERROR") {
            (RED, 5)
        } else if after_timestamp.starts_with("WARN ") {
            (YELLOW, 5)
        } else if after_timestamp.starts_with("INFO ") {
            (GREEN, 5)
        } else if after_timestamp.starts_with("DEBUG") {
            (CYAN, 5)
        } else if after_timestamp.starts_with("TRACE") {
            (MAGENTA, 5)
        } else {
            return line.to_string();
        };

        // Find the target section [target]
        let timestamp_end = level_start + 2;
        let level_section_end = timestamp_end + level_end;

        if let Some(target_start) = line[level_section_end..].find('[') {
            let target_abs_start = level_section_end + target_start;
            if let Some(target_end) = line[target_abs_start..].find(']') {
                let target_abs_end = target_abs_start + target_end + 1;

                // Build colorized string:
                // DIM[timestamp]RESET LEVEL_COLOR LEVEL RESET DIM[target]RESET message
                return format!(
                    "{}{}{}{}{}{}{}{}{}{}",
                    DIM,
                    &line[..timestamp_end], // [timestamp]
                    RESET,
                    level_color,
                    &line[timestamp_end..level_section_end], // LEVEL
                    RESET,
                    DIM,
                    &line[level_section_end..target_abs_end], // [target]
                    RESET,
                    &line[target_abs_end..] // message
                );
            }
        }
    }
    line.to_string()
}

impl Write for CustomWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Convert to string for colorization
        let text = String::from_utf8_lossy(buf);
        let colored = colorize_log_line(&text);

        // Print colored output to stdout
        let _ = io::stdout().write_all(colored.as_bytes());
        let _ = io::stdout().flush();

        // Write plain text to file
        match self.file.write_all(buf) {
            Ok(()) => {
                let _ = self.file.flush();
                Ok(buf.len())
            }
            Err(e) => {
                eprintln!("Failed to write to log file: {e}");
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stdout().flush();
        self.file.flush()
    }
}

fn cleanup_old_logs(log_dir: &PathBuf) -> Result<(), AppError> {
    let max_logs = 10;

    if let Ok(entries) = fs::read_dir(log_dir) {
        let mut log_files: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().is_some_and(|ext| ext == "log")
                    && entry
                        .path()
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("bmm_"))
            })
            .collect();

        // Sort files by modification time (oldest first)
        log_files.sort_by(|a, b| {
            let a_time = a
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or_else(|_| std::time::SystemTime::now());
            let b_time = b
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or_else(|_| std::time::SystemTime::now());
            a_time.cmp(&b_time)
        });

        // Remove older logs if we have more than max_logs
        if log_files.len() > max_logs {
            for old_file in log_files.iter().take(log_files.len() - max_logs) {
                let _ = fs::remove_file(old_file.path());
            }
        }
    }

    Ok(())
}
