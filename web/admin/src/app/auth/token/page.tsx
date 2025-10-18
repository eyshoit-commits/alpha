"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { TokenForm } from "@/components/token-form";

export default function TokenAuthPage() {
  const params = useSearchParams();
  const returnTo = params?.get("returnTo") ?? "/";

  return (
    <section className="mx-auto max-w-2xl space-y-6 rounded-lg border border-slate-200 bg-white p-8 shadow-sm">
      <header className="space-y-2">
        <p className="text-sm font-semibold uppercase tracking-wide text-slate-500">Authentication</p>
        <h2 className="text-2xl font-semibold">Provide an admin bearer token</h2>
        <p className="text-sm text-slate-600">
          Enter a valid admin-scoped token issued by the CAVE daemon. The token is stored in a SameSite=Strict cookie and reused
          for subsequent server-side requests. You will be redirected to <code className="rounded bg-slate-100 px-1">{returnTo}</code>
          after saving.
        </p>
      </header>
      <TokenForm showClear={false} />
      <p className="text-xs text-slate-500">
        Need to issue a key? Navigate to <Link className="underline" href="/keys">API Keys</Link> once authenticated.
      </p>
    </section>
  );
}
