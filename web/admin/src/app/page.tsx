export default function OverviewPage() {
  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h2 className="text-xl font-semibold">Phase-0 admin workflows</h2>
        <p className="mt-2 text-sm text-slate-600">
          Use the navigation links above to provision sandboxes, issue or revoke API keys, and review live telemetry.
          The console talks directly to the CAVE daemon via the documented <code className="rounded bg-slate-100 px-1">/api/v1</code>
          endpoints using the bearer token you provide.
        </p>
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <div className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm">
          <h3 className="text-lg font-semibold">Sandbox lifecycle</h3>
          <p className="mt-1 text-sm text-slate-600">
            Create, start, stop, delete and inspect sandboxes per namespace. Execution history is available from the
            sandbox detail drawer to help correlate runtime diagnostics.
          </p>
        </div>
        <div className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm">
          <h3 className="text-lg font-semibold">Key management</h3>
          <p className="mt-1 text-sm text-slate-600">
            View issued API keys, manage rate limits, rotate admin tokens, and revoke compromised credentials with
            audit-friendly key prefixes.
          </p>
        </div>
        <div className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm md:col-span-2">
          <h3 className="text-lg font-semibold">Telemetry snapshots</h3>
          <p className="mt-1 text-sm text-slate-600">
            Aggregate sandbox states and execution timings to validate SLOs. The telemetry view surfaces error rates, recent
            command latency, and utilization derived from daemon responses.
          </p>
        </div>
      </div>
    </section>
  );
}
