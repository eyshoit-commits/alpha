"use client";

import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { TokenForm } from "@/components/token-form";

export default function NamespaceTokenPage() {
  const params = useSearchParams();
  const returnTo = params?.get("returnTo") ?? "/";

  return (
    <section className="mx-auto max-w-2xl space-y-6 rounded-lg border border-slate-200 bg-white p-8 shadow-sm">
      <header className="space-y-2">
        <p className="text-sm font-semibold uppercase tracking-wide text-slate-500">Authentication</p>
        <h2 className="text-2xl font-semibold">Authenticate with a namespace token</h2>
        <p className="text-sm text-slate-600">
          Provide a namespace-scoped bearer token issued by the CAVE daemon. The token is stored in a SameSite=Strict cookie and
          reused on future server-side renders. After saving you will be redirected to
          <code className="mx-1 rounded bg-slate-100 px-1">{returnTo}</code>.
        </p>
      </header>
      <TokenForm showClear={false} />
      <p className="text-xs text-slate-500">
        Need to request access? Contact an administrator or visit the <Link className="underline" href="/">overview</Link> after
        authenticating.
      </p>
    </section>
  );
}
