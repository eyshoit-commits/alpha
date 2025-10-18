import type { Metadata } from "next";
import "./globals.css";
import Link from "next/link";
import { cookies } from "next/headers";
import { TokenProvider } from "../components/token-context";
import { TokenForm } from "../components/token-form";
import { NAMESPACE_TOKEN_COOKIE } from "@shared/auth";

export const metadata: Metadata = {
  title: "CAVE Namespace Portal",
  description: "Namespace operators can manage sandboxes and review execution history.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  const cookieStore = cookies();
  const initialToken = cookieStore.get(NAMESPACE_TOKEN_COOKIE)?.value ?? "";

  return (
    <html lang="en">
      <body>
        <TokenProvider initialToken={initialToken}>
          <div className="min-h-screen bg-slate-50 text-slate-900">
            <header className="border-b border-slate-200 bg-white">
              <div className="mx-auto flex max-w-5xl flex-col gap-4 px-6 py-6">
                <div className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
                  <div>
                    <h1 className="text-2xl font-semibold">CAVE Namespace Portal</h1>
                    <p className="text-sm text-slate-500">
                      Operate sandboxes, inspect execution history, and access documentation for best practices.
                    </p>
                  </div>
                  <TokenForm />
                </div>
                <nav className="flex flex-wrap gap-3 text-sm font-medium">
                  <Link className="rounded-md bg-slate-900 px-3 py-2 text-white shadow-sm hover:bg-slate-700" href="/">
                    Overview
                  </Link>
                  <Link className="rounded-md border border-slate-200 bg-white px-3 py-2 shadow-sm" href="/history">
                    Execution history
                  </Link>
                </nav>
              </div>
            </header>
            <main className="mx-auto max-w-5xl px-6 py-8">{children}</main>
          </div>
        </TokenProvider>
      </body>
    </html>
  );
}
