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
          <div className="min-h-screen bg-transparent text-slate-100">
            <header className="border-b-2 border-[#5bec92]/60 bg-gradient-to-r from-[#12172f] via-[#1a1f3a] to-[#12172f] px-6 py-4 shadow-lg shadow-[#5bec92]/30">
              <div className="mx-auto flex max-w-screen-2xl items-center justify-between">
                <div>
                  <h1 className="text-4xl font-black bg-gradient-to-r from-[#75ffaf] via-[#AF75FF] to-[#D3188C] bg-clip-text text-transparent tracking-tight">
                    ⚡ CAVE ADMIN
                  </h1>
                  <p className="text-sm text-slate-400 mt-1">Premium Sandbox Management Platform</p>
                </div>
                <TokenForm />
              </div>
              <nav className="mx-auto mt-4 flex max-w-screen-2xl gap-3">
                <Link href="/" className="group relative rounded-lg bg-gradient-to-r from-[#AF75FF] to-[#D3188C] px-5 py-2.5 text-base font-bold text-white shadow-lg shadow-[#D3188C]/50 hover:shadow-[#D3188C]/70 transition-all">
                  <span className="relative z-10">🏠 Overview</span>
                </Link>
                <Link href="/sandboxes" className="rounded-lg border-2 border-[#AF75FF]/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-[#AF75FF] hover:bg-[#AF75FF]/20 hover:text-[#AF75FF] hover:border-[#AF75FF] hover:shadow-lg hover:shadow-[#AF75FF]/30 transition-all">
                  📦 Sandboxes
                </Link>
                <Link href="/keys" className="rounded-lg border-2 border-[#75ffaf]/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-[#75ffaf] hover:bg-[#75ffaf]/20 hover:text-[#75ffaf] hover:border-[#75ffaf] hover:shadow-lg hover:shadow-[#75ffaf]/30 transition-all">
                  🔑 API Keys
                </Link>
                <Link href="/telemetry" className="rounded-lg border-2 border-[#EC5800]/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-[#EC5800] hover:bg-[#EC5800]/20 hover:text-[#EC5800] hover:border-[#EC5800] hover:shadow-lg hover:shadow-[#EC5800]/30 transition-all">
                  📊 Telemetry
                </Link>
                <Link href="/models" className="rounded-lg border-2 border-[#75ffaf]/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-[#75ffaf] hover:bg-[#75ffaf]/20 hover:text-[#75ffaf] hover:border-[#75ffaf] hover:shadow-lg hover:shadow-[#75ffaf]/30 transition-all">
                  🤖 Models
                </Link>
                <Link href="/audit" className="rounded-lg border-2 border-[#AF75FF]/50 bg-[#1a1f3a] px-5 py-2.5 text-sm font-bold text-[#AF75FF] hover:bg-[#AF75FF]/20 hover:text-[#AF75FF] hover:border-[#AF75FF] hover:shadow-lg hover:shadow-[#AF75FF]/30 transition-all">
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
