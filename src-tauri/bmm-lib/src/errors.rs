//! Application-wide error types for the Balatro Mod Manager.
//!
//! This module defines [`AppError`], a comprehensive error enum that covers
//! all error scenarios in the application, including:
//! - Database operations
//! - File system operations
//! - Network requests
//! - Mod installation and management
//! - Platform-specific issues
//!
//! The error type implements conversions from common error types and can be
//! easily converted to a String for Tauri command results.

use std::fmt;
use std::path::PathBuf;
use std::time::SystemTimeError;

/// Comprehensive error type for all application errors.
///
/// This enum provides detailed error variants for different failure scenarios,
/// enabling precise error handling and informative error messages.
#[derive(Debug)]
pub enum AppError {
    // Database errors
    DatabaseInit(String),
    DatabaseQuery(String),
    DatabaseTransaction(String),

    // Logging errors
    Logging(String),

    // File system errors
    FileRead {
        path: PathBuf,
        source: String,
    },
    FileCopy {
        source: String,
        dest: String,
        source_error: String,
    },
    FileWrite {
        path: PathBuf,
        source: String,
    },
    FileNotFound {
        path: PathBuf,
        source: String,
    },
    DirCreate {
        path: PathBuf,
        source: String,
    },
    DirNotFound(PathBuf),
    PathConversionError,

    // System errors
    SystemTime(String),
    ProcessExecution(String),

    // Application state
    LockPoisoned(String),
    InvalidState(String),

    // Mod management
    ModInstall {
        mod_name: String,
        source: String,
    },
    /// Reserved for future mod conflict detection
    #[allow(dead_code)]
    ModConflict {
        mod_name: String,
        conflicts: Vec<String>,
    },
    ModNotFound {
        mod_name: String,
        version: String,
    },
    /// Reserved for future git-based mod repository features
    #[allow(dead_code)]
    GitOperation(String),
    ArchiveTooLarge {
        reason: String,
    },

    // Network/API
    NetworkRequest {
        url: String,
        source: String,
    },
    ApiLimitExceeded,
    InvalidApiResponse(String),

    // Platform specific
    /// Reserved for future macOS-specific library loading features
    #[allow(dead_code)]
    MacOsLibrary {
        lib_name: String,
        source: String,
    },
    SystemDetection(String),
    UnsupportedArchitecture(String),

    // Configuration
    InvalidConfig {
        key: String,
        value: String,
    },
    PathValidation {
        path: PathBuf,
        reason: String,
    },

    // UI/Window
    WindowCreation(String),
    DialogError(String),

    // Serialization
    Serialization {
        format: String,
        source: String,
    },
    JsonParse {
        path: PathBuf,
        source: String,
    },

    // Network
    Network(String),

    // Miscellaneous
    Unknown(String),
}
// │   │   required for `Result<Vec<Mod>, AppError>` to implement `FromResidual<Result<Infallible, ParseBoolError>>` rustc (E0277) [125, 68]

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Database errors
            AppError::DatabaseInit(msg) => write!(f, "Database initialization failed: {msg}"),
            AppError::DatabaseQuery(msg) => write!(f, "Database error: {msg}"),
            AppError::DatabaseTransaction(msg) => write!(f, "Database transaction failed: {msg}"),

            // Logging errors
            AppError::Logging(msg) => write!(f, "Logging error: {msg}"),

            // File system errors
            AppError::FileRead { path, source } => {
                write!(f, "Failed to read file '{}': {}", path.display(), source)
            }
            AppError::FileCopy {
                source,
                dest,
                source_error,
            } => {
                write!(
                    f,
                    "Failed to copy '{}' to '{}': {}",
                    source, dest, source_error
                )
            }
            AppError::FileWrite { path, source } => {
                write!(f, "Failed to write file '{}': {}", path.display(), source)
            }
            AppError::FileNotFound { path, source } => {
                write!(f, "File not found '{}': {}", path.display(), source)
            }
            AppError::DirCreate { path, source } => {
                write!(
                    f,
                    "Failed to create directory '{}': {}",
                    path.display(),
                    source
                )
            }
            AppError::DirNotFound(path) => {
                write!(f, "Directory not found: '{}'", path.display())
            }
            AppError::PathConversionError => {
                write!(f, "Failed to convert path to string")
            }

            // System errors
            AppError::SystemTime(msg) => write!(f, "System time error: {msg}"),
            AppError::ProcessExecution(msg) => write!(f, "Process execution failed: {msg}"),

            // Application state
            AppError::LockPoisoned(msg) => write!(f, "Internal lock error: {msg}"),
            AppError::InvalidState(msg) => write!(f, "{msg}"),

            // Mod management
            AppError::ModInstall { mod_name, source } => {
                write!(f, "Failed to install mod '{mod_name}': {source}")
            }
            AppError::ModConflict {
                mod_name,
                conflicts,
            } => {
                write!(
                    f,
                    "Mod '{}' conflicts with: {}",
                    mod_name,
                    conflicts.join(", ")
                )
            }
            AppError::ModNotFound { mod_name, version } => {
                if version.is_empty() {
                    write!(f, "Mod '{}' not found", mod_name)
                } else {
                    write!(f, "Mod '{}' version '{}' not found", mod_name, version)
                }
            }
            AppError::GitOperation(msg) => write!(f, "Git operation failed: {msg}"),
            AppError::ArchiveTooLarge { reason } => {
                write!(f, "Archive extraction aborted: {reason}")
            }

            // Network/API
            AppError::NetworkRequest { url: _, source } => {
                // Show only the underlying message to keep UI errors concise
                write!(f, "{source}")
            }
            AppError::ApiLimitExceeded => {
                write!(f, "API rate limit exceeded. Please try again later.")
            }
            AppError::InvalidApiResponse(msg) => write!(f, "Invalid API response: {msg}"),

            // Platform specific
            AppError::MacOsLibrary { lib_name, source } => {
                write!(f, "macOS library '{lib_name}' error: {source}")
            }
            AppError::SystemDetection(msg) => write!(f, "System detection failed: {msg}"),
            AppError::UnsupportedArchitecture(arch) => {
                write!(f, "Unsupported system architecture: {arch}")
            }

            // Configuration
            AppError::InvalidConfig { key, value } => {
                write!(f, "Invalid configuration for '{}': {}", key, value)
            }
            AppError::PathValidation { path, reason } => {
                write!(f, "Invalid path '{}': {}", path.display(), reason)
            }

            // UI/Window
            AppError::WindowCreation(msg) => write!(f, "Window creation failed: {msg}"),
            AppError::DialogError(msg) => write!(f, "Dialog error: {msg}"),

            // Serialization
            AppError::Serialization { format, source } => {
                write!(f, "Failed to process {} data: {}", format, source)
            }
            AppError::JsonParse { path, source } => {
                write!(f, "Failed to parse JSON '{}': {}", path.display(), source)
            }

            // Network
            AppError::Network(msg) => write!(f, "Network error: {msg}"),

            // Miscellaneous
            AppError::Unknown(msg) => write!(f, "An error occurred: {msg}"),
        }
    }
}

impl From<std::convert::Infallible> for AppError {
    fn from(_: std::convert::Infallible) -> Self {
        AppError::Unknown("Infallible error occurred".to_string())
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None // Implement if needed for error chaining
    }
}

// Conversion implementations
impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::DatabaseQuery(err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::FileRead {
            path: PathBuf::from("unknown"),
            source: err.to_string(),
        }
    }
}

impl From<SystemTimeError> for AppError {
    fn from(err: SystemTimeError) -> Self {
        AppError::SystemTime(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::NetworkRequest {
            url: err.url().map(|u| u.to_string()).unwrap_or_default(),
            source: err.to_string(),
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        AppError::WindowCreation(err.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Unknown(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization {
            format: "JSON".to_string(),
            source: err.to_string(),
        }
    }
}

impl<T> From<std::sync::PoisonError<T>> for AppError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        AppError::LockPoisoned(format!("Mutex poison error: {err}"))
    }
}

// For Tauri command result compatibility
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        format!("{err}")
    }
}

// Additional constructors
impl AppError {
    pub fn invalid_path(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        AppError::PathValidation {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub fn mod_install_error(mod_name: impl Into<String>, source: impl Into<String>) -> Self {
        AppError::ModInstall {
            mod_name: mod_name.into(),
            source: source.into(),
        }
    }

    pub fn config_error(key: impl Into<String>, value: impl Into<String>) -> Self {
        AppError::InvalidConfig {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Convert to a frontend-friendly error with categorization.
    /// This allows the frontend to show appropriate UI based on error type.
    pub fn to_frontend_error(&self) -> FrontendError {
        match self {
            // Network errors
            AppError::NetworkRequest { url, source } => FrontendError {
                category: ErrorCategory::Network,
                message: source.clone(),
                details: Some(format!("URL: {}", url)),
                retryable: true,
            },
            AppError::ApiLimitExceeded => FrontendError {
                category: ErrorCategory::RateLimit,
                message: "API rate limit exceeded. Please try again later.".to_string(),
                details: None,
                retryable: true,
            },
            AppError::Network(msg) => FrontendError {
                category: ErrorCategory::Network,
                message: msg.clone(),
                details: None,
                retryable: true,
            },

            // File system errors
            AppError::FileRead { path, source } => FrontendError {
                category: ErrorCategory::FileSystem,
                message: format!("Failed to read file: {}", source),
                details: Some(path.display().to_string()),
                retryable: false,
            },
            AppError::FileWrite { path, source } => FrontendError {
                category: ErrorCategory::FileSystem,
                message: format!("Failed to write file: {}", source),
                details: Some(path.display().to_string()),
                retryable: false,
            },
            AppError::DirNotFound(path) => FrontendError {
                category: ErrorCategory::FileSystem,
                message: format!("Directory not found: {}", path.display()),
                details: None,
                retryable: false,
            },

            // Database errors
            AppError::DatabaseInit(msg)
            | AppError::DatabaseQuery(msg)
            | AppError::DatabaseTransaction(msg) => FrontendError {
                category: ErrorCategory::Database,
                message: msg.clone(),
                details: None,
                retryable: false,
            },

            // Mod installation errors
            AppError::ModInstall { mod_name, source } => FrontendError {
                category: ErrorCategory::ModInstall,
                message: format!("Failed to install {}: {}", mod_name, source),
                details: None,
                retryable: true,
            },
            AppError::ModNotFound { mod_name, version } => FrontendError {
                category: ErrorCategory::ModInstall,
                message: if version.is_empty() {
                    format!("Mod '{}' not found", mod_name)
                } else {
                    format!("Mod '{}' version '{}' not found", mod_name, version)
                },
                details: None,
                retryable: false,
            },

            // Default fallback
            _ => FrontendError {
                category: ErrorCategory::Unknown,
                message: self.to_string(),
                details: None,
                retryable: false,
            },
        }
    }
}

/// Error categories for frontend UI handling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Network connectivity or request failures
    Network,
    /// API rate limit exceeded
    RateLimit,
    /// File system read/write errors
    FileSystem,
    /// Database operation failures
    Database,
    /// Mod installation failures
    ModInstall,
    /// Unknown or uncategorized errors
    Unknown,
}

/// Frontend-friendly error representation with categorization.
/// Allows the UI to show appropriate feedback based on error type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FrontendError {
    /// Category for UI handling (determines icon, color, etc.)
    pub category: ErrorCategory,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details (file path, URL, etc.)
    pub details: Option<String>,
    /// Whether the operation can be retried
    pub retryable: bool,
}

impl std::fmt::Display for FrontendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_init_display() {
        let err = AppError::DatabaseInit("connection failed".to_string());
        assert_eq!(
            err.to_string(),
            "Database initialization failed: connection failed"
        );
    }

    #[test]
    fn test_database_query_display() {
        let err = AppError::DatabaseQuery("syntax error".to_string());
        assert_eq!(err.to_string(), "Database error: syntax error");
    }

    #[test]
    fn test_file_read_display() {
        let err = AppError::FileRead {
            path: PathBuf::from("/test/file.txt"),
            source: "permission denied".to_string(),
        };
        assert!(err.to_string().contains("/test/file.txt"));
        assert!(err.to_string().contains("permission denied"));
    }

    #[test]
    fn test_file_copy_display() {
        let err = AppError::FileCopy {
            source: "/src".to_string(),
            dest: "/dst".to_string(),
            source_error: "disk full".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("/src"));
        assert!(msg.contains("/dst"));
        assert!(msg.contains("disk full"));
    }

    #[test]
    fn test_dir_not_found_display() {
        let err = AppError::DirNotFound(PathBuf::from("/missing/dir"));
        assert!(err.to_string().contains("/missing/dir"));
    }

    #[test]
    fn test_mod_install_display() {
        let err = AppError::ModInstall {
            mod_name: "Steamodded".to_string(),
            source: "download failed".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Steamodded"));
        assert!(msg.contains("download failed"));
    }

    #[test]
    fn test_mod_not_found_with_version() {
        let err = AppError::ModNotFound {
            mod_name: "TestMod".to_string(),
            version: "1.0.0".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("TestMod"));
        assert!(msg.contains("1.0.0"));
    }

    #[test]
    fn test_mod_not_found_without_version() {
        let err = AppError::ModNotFound {
            mod_name: "TestMod".to_string(),
            version: String::new(),
        };
        let msg = err.to_string();
        assert!(msg.contains("TestMod"));
        assert!(!msg.contains("version"));
    }

    #[test]
    fn test_api_limit_exceeded_display() {
        let err = AppError::ApiLimitExceeded;
        assert!(err.to_string().contains("rate limit"));
    }

    #[test]
    fn test_archive_too_large_display() {
        let err = AppError::ArchiveTooLarge {
            reason: "exceeds 100MB limit".to_string(),
        };
        assert!(err.to_string().contains("100MB"));
    }

    #[test]
    fn test_invalid_path_constructor() {
        let err = AppError::invalid_path("/bad/path", "contains invalid characters");
        match err {
            AppError::PathValidation { path, reason } => {
                assert_eq!(path, PathBuf::from("/bad/path"));
                assert_eq!(reason, "contains invalid characters");
            }
            _ => panic!("Expected PathValidation variant"),
        }
    }

    #[test]
    fn test_mod_install_error_constructor() {
        let err = AppError::mod_install_error("MyMod", "network timeout");
        match err {
            AppError::ModInstall { mod_name, source } => {
                assert_eq!(mod_name, "MyMod");
                assert_eq!(source, "network timeout");
            }
            _ => panic!("Expected ModInstall variant"),
        }
    }

    #[test]
    fn test_config_error_constructor() {
        let err = AppError::config_error("theme", "invalid value");
        match err {
            AppError::InvalidConfig { key, value } => {
                assert_eq!(key, "theme");
                assert_eq!(value, "invalid value");
            }
            _ => panic!("Expected InvalidConfig variant"),
        }
    }

    #[test]
    fn test_from_rusqlite_error() {
        let sqlite_err = rusqlite::Error::InvalidQuery;
        let app_err: AppError = sqlite_err.into();
        match app_err {
            AppError::DatabaseQuery(_) => {}
            _ => panic!("Expected DatabaseQuery variant"),
        }
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let app_err: AppError = io_err.into();
        match app_err {
            AppError::FileRead { .. } => {}
            _ => panic!("Expected FileRead variant"),
        }
    }

    #[test]
    fn test_app_error_to_string_conversion() {
        let err = AppError::Unknown("test error".to_string());
        let s: String = err.into();
        assert!(s.contains("test error"));
    }

    #[test]
    fn test_frontend_error_network_category() {
        let err = AppError::NetworkRequest {
            url: "https://example.com".to_string(),
            source: "connection refused".to_string(),
        };
        let frontend = err.to_frontend_error();
        assert_eq!(frontend.category, ErrorCategory::Network);
        assert!(frontend.retryable);
        assert!(frontend.details.is_some());
    }

    #[test]
    fn test_frontend_error_rate_limit() {
        let err = AppError::ApiLimitExceeded;
        let frontend = err.to_frontend_error();
        assert_eq!(frontend.category, ErrorCategory::RateLimit);
        assert!(frontend.retryable);
    }

    #[test]
    fn test_frontend_error_filesystem() {
        let err = AppError::DirNotFound(PathBuf::from("/missing"));
        let frontend = err.to_frontend_error();
        assert_eq!(frontend.category, ErrorCategory::FileSystem);
        assert!(!frontend.retryable);
    }

    #[test]
    fn test_frontend_error_database() {
        let err = AppError::DatabaseQuery("syntax error".to_string());
        let frontend = err.to_frontend_error();
        assert_eq!(frontend.category, ErrorCategory::Database);
        assert!(!frontend.retryable);
    }

    #[test]
    fn test_frontend_error_mod_install() {
        let err = AppError::ModInstall {
            mod_name: "Test".to_string(),
            source: "failed".to_string(),
        };
        let frontend = err.to_frontend_error();
        assert_eq!(frontend.category, ErrorCategory::ModInstall);
        assert!(frontend.retryable);
    }

    #[test]
    fn test_frontend_error_unknown_fallback() {
        let err = AppError::WindowCreation("window failed".to_string());
        let frontend = err.to_frontend_error();
        assert_eq!(frontend.category, ErrorCategory::Unknown);
    }

    #[test]
    fn test_frontend_error_display() {
        let frontend = FrontendError {
            category: ErrorCategory::Network,
            message: "Connection failed".to_string(),
            details: None,
            retryable: true,
        };
        assert_eq!(frontend.to_string(), "Connection failed");
    }
}
