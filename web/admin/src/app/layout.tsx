import type { Metadata } from "next";
import "./globals.css";
import Link from "next/link";
import { TokenProvider } from "../components/token-context";
import { TokenForm } from "../components/token-form";

export const metadata: Metadata = {
  title: "CAVE Admin Console",
  description: "Administer sandboxes, API keys, and telemetry for the CAVE daemon.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <TokenProvider>
          <div className="min-h-screen bg-slate-50 text-slate-900">
            <header className="border-b border-slate-200 bg-white">
              <div className="mx-auto flex max-w-6xl flex-col gap-4 px-6 py-6">
                <div className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
                  <div>
                    <h1 className="text-2xl font-semibold">CAVE Admin Console</h1>
                    <p className="text-sm text-slate-500">
                      Monitor and manage daemon sandboxes, API keys, and telemetry insights.
                    </p>
                  </div>
                  <TokenForm />
                </div>
                <nav className="flex flex-wrap gap-3 text-sm font-medium">
                  <Link className="rounded-md bg-slate-900 px-3 py-2 text-white shadow-sm hover:bg-slate-700" href="/">
                    Overview
                  </Link>
                  <Link className="rounded-md border border-slate-200 bg-white px-3 py-2 shadow-sm" href="/sandboxes">
                    Sandboxes
                  </Link>
                  <Link className="rounded-md border border-slate-200 bg-white px-3 py-2 shadow-sm" href="/keys">
                    API Keys
                  </Link>
                  <Link className="rounded-md border border-slate-200 bg-white px-3 py-2 shadow-sm" href="/telemetry">
                    Telemetry
                  </Link>
                </nav>
              </div>
            </header>
            <main className="mx-auto max-w-6xl px-6 py-8">{children}</main>
          </div>
        </TokenProvider>
      </body>
    </html>
  );
}
