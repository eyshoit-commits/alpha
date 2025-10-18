export default function OverviewPage() {
  return (
    <section className="space-y-6">
      <div className="rounded-xl border-2 border-[#A142B3]/50 bg-[#12172f] p-6 shadow-lg shadow-[#A142B3]/20 hover:border-[#A142B3]/70 hover:shadow-[#A142B3]/30 transition-all">
        <h2 className="text-xl font-bold text-[#75ffaf]">Phase-0 admin workflows</h2>
        <p className="mt-2 text-base text-slate-300">
          Use the navigation links above to provision sandboxes, issue or revoke API keys, and review live telemetry.
          The console talks directly to the CAVE daemon via the documented <code className="rounded bg-[#1a1f3a] px-2 py-1 text-[#AF75FF] border border-[#AF75FF]/30">/api/v1</code>
          endpoints using the bearer token you provide.
        </p>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <div className="rounded-xl border-2 border-[#A142B3]/50 bg-[#12172f] p-5 shadow-lg shadow-[#A142B3]/20 hover:border-[#A142B3]/70 hover:shadow-[#A142B3]/30 transition-all">
          <h3 className="text-lg font-bold text-[#AF75FF]">📦 Sandbox lifecycle</h3>
          <p className="mt-2 text-sm text-slate-300">
            Create, start, stop, delete and inspect sandboxes per namespace. Execution history is available from the
            sandbox detail drawer to help correlate runtime diagnostics.
          </p>
        </div>
        <div className="rounded-xl border-2 border-[#A142B3]/50 bg-[#12172f] p-5 shadow-lg shadow-[#A142B3]/20 hover:border-[#A142B3]/70 hover:shadow-[#A142B3]/30 transition-all">
          <h3 className="text-lg font-bold text-[#AF75FF]">🔑 Key management</h3>
          <p className="mt-2 text-sm text-slate-300">
            View issued API keys, manage rate limits, rotate admin tokens, and revoke compromised credentials with
            audit-friendly key prefixes.
          </p>
        </div>
        <div className="rounded-xl border-2 border-[#A142B3]/50 bg-[#12172f] p-5 shadow-lg shadow-[#A142B3]/20 hover:border-[#A142B3]/70 hover:shadow-[#A142B3]/30 transition-all md:col-span-2">
          <h3 className="text-lg font-bold text-[#AF75FF]">📊 Telemetry snapshots</h3>
          <p className="mt-2 text-sm text-slate-300">
            Aggregate sandbox states and execution timings to validate SLOs. The telemetry view surfaces error rates, recent
            command latency, and utilization derived from daemon responses.
          </p>
        </div>
      </div>
    </section>
  );
}
