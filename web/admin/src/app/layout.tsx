import type { Metadata } from "next";
import "./globals.css";
import Link from "next/link";
import { cookies } from "next/headers";
import { TokenProvider } from "../components/token-context";
import { TokenForm } from "../components/token-form";
import { ADMIN_TOKEN_COOKIE } from "@shared/auth";

export const metadata: Metadata = {
  title: "CAVE Admin Console",
  description: "Administer sandboxes, API keys, and telemetry for the CAVE daemon.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  const cookieStore = cookies();
  const initialToken = cookieStore.get(ADMIN_TOKEN_COOKIE)?.value ?? "";

  return (
    <html lang="en">
      <body>
        <TokenProvider initialToken={initialToken}>
          <div className="min-h-screen bg-slate-50 text-slate-900">
            <header className="border-b-2 border-purple-500/30 bg-gradient-to-r from-[#12172f] via-[#1a1f3a] to-[#12172f] px-6 py-4 shadow-lg shadow-purple-500/20">
              <div className="mx-auto flex max-w-screen-2xl items-center justify-between">
                <div>
                  <h1 className="text-3xl font-black bg-gradient-to-r from-cyan-400 via-purple-400 to-pink-400 bg-clip-text text-transparent tracking-tight">
                    ⚡ CAVE ADMIN
                  </h1>
                  <p className="text-sm text-slate-400 mt-1">Premium Sandbox Management Platform</p>
                </div>
                <TokenForm />
              </div>
              <nav className="mx-auto mt-4 flex max-w-screen-2xl gap-3">
                <Link href="/" className="group relative rounded-lg bg-gradient-to-r from-purple-600 to-pink-600 px-5 py-2.5 text-sm font-bold text-white shadow-lg shadow-purple-500/50 hover:shadow-purple-400/60 transition-all">
                  <span className="relative z-10">🏠 Overview</span>
                </Link>
                <Link href="/sandboxes" className="rounded-lg border-2 border-purple-500/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-purple-300 hover:bg-purple-500/20 hover:text-purple-200 hover:border-purple-400 hover:shadow-lg hover:shadow-purple-500/30 transition-all">
                  📦 Sandboxes
                </Link>
                <Link href="/keys" className="rounded-lg border-2 border-cyan-500/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-cyan-300 hover:bg-cyan-500/20 hover:text-cyan-200 hover:border-cyan-400 hover:shadow-lg hover:shadow-cyan-500/30 transition-all">
                  🔑 API Keys
                </Link>
                <Link href="/telemetry" className="rounded-lg border-2 border-pink-500/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-pink-300 hover:bg-pink-500/20 hover:text-pink-200 hover:border-pink-400 hover:shadow-lg hover:shadow-pink-500/30 transition-all">
                  📊 Telemetry
                </Link>
                <Link href="/models" className="rounded-lg border-2 border-blue-500/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-blue-300 hover:bg-blue-500/20 hover:text-blue-200 hover:border-blue-400 hover:shadow-lg hover:shadow-blue-500/30 transition-all">
                  🤖 Models
                </Link>
                <Link href="/audit" className="rounded-lg border-2 border-green-500/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-green-300 hover:bg-green-500/20 hover:text-green-200 hover:border-green-400 hover:shadow-lg hover:shadow-green-500/30 transition-all">
                  📜 Audit
                </Link>
              </nav>
            </header>
            <main className="mx-auto max-w-screen-2xl">{children}</main>
          </div>
        </TokenProvider>
      </body>
    </html>
  );
}
