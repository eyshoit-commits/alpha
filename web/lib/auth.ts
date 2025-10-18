export const ADMIN_TOKEN_COOKIE = "cave-admin-token";
export const NAMESPACE_TOKEN_COOKIE = "cave-namespace-token";

const MAX_AGE_SECONDS = 7 * 24 * 60 * 60; // 7 days

function resolveCookieAttributes(): string {
  const attributes = ["path=/", "SameSite=Strict"];
  if (typeof window !== "undefined" && window.location.protocol === "https:") {
    attributes.push("Secure");
  }
  attributes.push(`Max-Age=${MAX_AGE_SECONDS}`);
  return attributes.join("; ");
}

export function writeTokenCookie(name: string, value: string) {
  if (typeof document === "undefined") {
    return;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    deleteTokenCookie(name);
    return;
  }
  document.cookie = `${name}=${encodeURIComponent(trimmed)}; ${resolveCookieAttributes()}`;
}

export function deleteTokenCookie(name: string) {
  if (typeof document === "undefined") {
    return;
  }
  document.cookie = `${name}=; path=/; SameSite=Strict; Max-Age=0`;
}

export function readTokenCookie(name: string): string {
  if (typeof document === "undefined") {
    return "";
  }
  const entries = document.cookie.split(";").map((entry) => entry.trim());
  for (const entry of entries) {
    if (!entry) continue;
    const [key, ...rest] = entry.split("=");
    if (key === name) {
      return decodeURIComponent(rest.join("="));
    }
  }
  return "";
}

export function buildContentSecurityPolicy(origin = "self"): string {
  const connectSources = new Set(["'self'"]);
  if (origin && origin !== "self") {
    connectSources.add(origin);
  }
  return [
    "default-src 'self'",
    "frame-ancestors 'none'",
    "form-action 'self'",
    "base-uri 'self'",
    "img-src 'self' data:",
    "font-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    `connect-src ${Array.from(connectSources).join(" ")}`,
    "script-src 'self'",
    "object-src 'none'",
    "upgrade-insecure-requests",
  ].join("; ");
}
