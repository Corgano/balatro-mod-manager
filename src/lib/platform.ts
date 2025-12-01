import { browser } from "$app/environment";

let cachedIsLinux: boolean | null = null;

export async function isLinuxPlatform(): Promise<boolean> {
  if (cachedIsLinux !== null) return cachedIsLinux;

  try {
    const { platform } = await import("@tauri-apps/plugin-os");
    cachedIsLinux = (await platform()).toLowerCase() === "linux";
    return cachedIsLinux;
  } catch {
    // Fallback to user agent detection when plugin-os is unavailable (web preview)
    if (browser) {
      const ua = navigator.userAgent.toLowerCase();
      cachedIsLinux = ua.includes("linux") && !ua.includes("android");
      return cachedIsLinux;
    }
  }

  cachedIsLinux = false;
  return cachedIsLinux;
}
