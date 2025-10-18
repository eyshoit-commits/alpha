import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { ADMIN_TOKEN_COOKIE, buildContentSecurityPolicy } from "@shared/auth";
import { DEFAULT_DAEMON_URL } from "@shared/api";

const PUBLIC_PATHS = new Set(["/auth/token"]);

function applySecurityHeaders(response: NextResponse) {
  const daemonOrigin = process.env.NEXT_PUBLIC_DAEMON_URL ?? DEFAULT_DAEMON_URL;
  response.headers.set("Content-Security-Policy", buildContentSecurityPolicy(daemonOrigin));
  response.headers.set("Referrer-Policy", "no-referrer");
  response.headers.set("X-Content-Type-Options", "nosniff");
  response.headers.set("X-Frame-Options", "DENY");
  response.headers.set("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
}

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  if (pathname.startsWith("/_next") || pathname.startsWith("/static")) {
    return NextResponse.next();
  }

  if (pathname.startsWith("/api/")) {
    return NextResponse.next();
  }

  const hasToken = Boolean(request.cookies.get(ADMIN_TOKEN_COOKIE)?.value);

  if (!hasToken && !PUBLIC_PATHS.has(pathname)) {
    const url = request.nextUrl.clone();
    url.pathname = "/auth/token";
    if (pathname !== "/") {
      url.searchParams.set("returnTo", pathname);
    }
    const redirect = NextResponse.redirect(url);
    applySecurityHeaders(redirect);
    return redirect;
  }

  const response = NextResponse.next();
  applySecurityHeaders(response);
  return response;
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico|robots.txt|manifest.webmanifest).*)"],
};
