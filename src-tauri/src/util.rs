use bmm_lib::errors::AppError;

/// Map library `AppError` to a string for Tauri command results.
pub fn map_error<T>(result: Result<T, AppError>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_map_error_ok() {
        let result: Result<i32, AppError> = Ok(42);
        let mapped = map_error(result);
        assert_eq!(mapped, Ok(42));
    }

    #[test]
    fn test_map_error_err() {
        let result: Result<i32, AppError> = Err(AppError::DirNotFound(PathBuf::from("/test")));
        let mapped = map_error(result);
        assert!(mapped.is_err());
        let err_str = mapped.unwrap_err();
        assert!(err_str.contains("/test") || err_str.contains("not found"));
    }

    #[test]
    fn test_map_error_preserves_value() {
        let result: Result<String, AppError> = Ok("hello".to_string());
        let mapped = map_error(result);
        assert_eq!(mapped, Ok("hello".to_string()));
    }
}
