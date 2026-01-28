/**
 * Error mapping utility for user-friendly error messages.
 * Maps technical error messages to actionable, understandable text.
 */

interface ErrorMapping {
  pattern: RegExp | string;
  message: string;
  action?: string;
}

const errorMappings: ErrorMapping[] = [
  // Network errors
  {
    pattern: /failed to fetch|networkerror|econnrefused|enotfound/i,
    message: "Unable to connect to the server",
    action: "Check your internet connection and try again.",
  },
  {
    pattern: /timeout|timed out/i,
    message: "The request timed out",
    action: "The server may be busy. Please try again later.",
  },
  {
    pattern: /rate limit|429|too many requests/i,
    message: "Too many requests",
    action: "Please wait a moment before trying again.",
  },
  {
    pattern: /offline/i,
    message: "You are offline",
    action: "Connect to the internet to access online features.",
  },

  // File/path errors
  {
    pattern: /file not found|no such file|enoent/i,
    message: "File not found",
    action: "The file may have been moved or deleted.",
  },
  {
    pattern: /permission denied|eacces|eperm/i,
    message: "Permission denied",
    action: "Try running the app with administrator privileges.",
  },
  {
    pattern: /disk full|no space|enospc/i,
    message: "Not enough disk space",
    action: "Free up some disk space and try again.",
  },
  {
    pattern: /directory not found/i,
    message: "Directory not found",
    action: "The folder may have been moved or deleted.",
  },

  // Balatro-specific errors
  {
    pattern: /balatro not found/i,
    message: "Balatro installation not found",
    action: "Make sure Balatro is installed and select the correct path.",
  },
  {
    pattern: /steam not found|steam installation/i,
    message: "Steam installation not found",
    action: "Make sure Steam is installed or manually select the Balatro path.",
  },
  {
    pattern: /invalid.*path/i,
    message: "Invalid path selected",
    action: "Please select a valid Balatro installation folder.",
  },

  // Mod installation errors
  {
    pattern: /download.*failed|failed.*download/i,
    message: "Download failed",
    action: "Check your internet connection and try again.",
  },
  {
    pattern: /extraction failed|unzip|decompress/i,
    message: "Failed to extract mod files",
    action: "The download may be corrupted. Try again.",
  },
  {
    pattern: /already installed/i,
    message: "Mod is already installed",
    action: "Uninstall the existing version first if you want to reinstall.",
  },
  {
    pattern: /dependency|requires steamodded|requires talisman/i,
    message: "Missing dependency",
    action: "Install the required dependencies first.",
  },

  // Database errors
  {
    pattern: /database|sqlite|rusqlite/i,
    message: "Database error",
    action: "Try restarting the app. If the problem persists, reset the cache.",
  },

  // Generic server errors
  {
    pattern: /500|internal server error/i,
    message: "Server error",
    action: "The mod server may be experiencing issues. Try again later.",
  },
  {
    pattern: /404|not found/i,
    message: "Resource not found",
    action: "The requested item may no longer be available.",
  },
  {
    pattern: /503|service unavailable/i,
    message: "Service temporarily unavailable",
    action: "The server is under maintenance. Try again later.",
  },
];

export interface FriendlyError {
  message: string;
  action?: string;
  technical?: string;
}

/**
 * Convert a technical error to a user-friendly message.
 */
export function toFriendlyError(error: unknown): FriendlyError {
  const errorMessage = extractErrorMessage(error);

  for (const mapping of errorMappings) {
    const matches =
      typeof mapping.pattern === "string"
        ? errorMessage.toLowerCase().includes(mapping.pattern.toLowerCase())
        : mapping.pattern.test(errorMessage);

    if (matches) {
      return {
        message: mapping.message,
        action: mapping.action,
        technical: errorMessage,
      };
    }
  }

  // No mapping found - return a generic message with the original error
  return {
    message: "An unexpected error occurred",
    action: "Please try again. If the problem persists, restart the app.",
    technical: errorMessage,
  };
}

/**
 * Extract error message from various error types.
 */
export function extractErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

/**
 * Format error for display - returns just the user-friendly message.
 */
export function formatError(error: unknown): string {
  const friendly = toFriendlyError(error);
  return friendly.action
    ? `${friendly.message}. ${friendly.action}`
    : friendly.message;
}

/**
 * Format error with technical details for logging/debugging.
 */
export function formatErrorWithDetails(error: unknown): string {
  const friendly = toFriendlyError(error);
  let result = friendly.message;
  if (friendly.action) {
    result += `. ${friendly.action}`;
  }
  if (friendly.technical && friendly.technical !== friendly.message) {
    result += ` (${friendly.technical})`;
  }
  return result;
}
