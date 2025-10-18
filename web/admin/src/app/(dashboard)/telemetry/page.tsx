"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { ApiClient, DaemonApiError, ExecutionRecord, sharedApiClient } from "@shared/api";
import { useToken } from "@/components/token-context";

type TelemetrySnapshot = {
  namespace: string;
  total: number;
  running: number;
  pending: number;
  failed: number;
  avgDurationMs: number | null;
  recentErrors: ExecutionRecord[];
};

export default function TelemetryPage() {
  const { token } = useToken();
  const client: ApiClient | null = useMemo(() => (token ? sharedApiClient.withToken(token) : null), [token]);
  const [namespace, setNamespace] = useState("default");
  const [snapshot, setSnapshot] = useState<TelemetrySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const loadTelemetry = useCallback(async () => {
    if (!client || !namespace) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const sandboxes = await client.listSandboxes(namespace);
      const running = sandboxes.filter((item) => item.status === "running").length;
      const pending = sandboxes.filter((item) => item.status === "pending").length;
      const failed = sandboxes.filter((item) => item.status === "failed").length;

      const executionRequests = sandboxes.map((sandbox) => client.listExecutions(sandbox.id, 5));
      const histories = await Promise.all(executionRequests);
      const flat = histories.flat();
      const completed = flat.filter((record) => record.exit_code !== null);
      const durations = completed.map((record) => record.duration_ms);
      const avgDurationMs = durations.length ? Math.round(durations.reduce((a, b) => a + b, 0) / durations.length) : null;
      const recentErrors = flat.filter((record) => record.exit_code && record.exit_code !== 0).slice(0, 5);

      setSnapshot({
        namespace,
        total: sandboxes.length,
        running,
        pending,
        failed,
        avgDurationMs,
        recentErrors,
      });
    } catch (err) {
      setError(extractErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [client, namespace]);

  useEffect(() => {
    void loadTelemetry();
  }, [loadTelemetry]);

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h2 className="text-xl font-semibold">Namespace telemetry</h2>
        <p className="mt-2 text-sm text-slate-600">
          Telemetry pulls live data from the sandbox and execution APIs to summarize utilization, latency, and failure rates.
          Use it to verify service level objectives and spot runaway executions.
        </p>
        <div className="mt-4 flex flex-wrap gap-3 text-sm">
          <label className="flex flex-col">
            <span className="font-medium">Namespace</span>
            <input
              value={namespace}
              onChange={(event) => setNamespace(event.target.value)}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="default"
            />
          </label>
          <button
            onClick={loadTelemetry}
            className="self-end rounded-md border border-slate-200 bg-white px-4 py-2 text-sm shadow-sm"
          >
            Refresh snapshot
          </button>
        </div>
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
      </div>

      {snapshot && (
        <div className="grid gap-4 md:grid-cols-2">
          <div className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm">
            <h3 className="text-lg font-semibold">Sandbox utilization</h3>
            <dl className="mt-3 grid grid-cols-2 gap-3 text-sm">
              <Stat label="Total" value={snapshot.total.toString()} />
              <Stat label="Running" value={snapshot.running.toString()} />
              <Stat label="Pending" value={snapshot.pending.toString()} />
              <Stat label="Failed" value={snapshot.failed.toString()} />
              <Stat label="Avg duration (ms)" value={snapshot.avgDurationMs?.toString() ?? "n/a"} />
            </dl>
          </div>
          <div className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm">
            <h3 className="text-lg font-semibold">Recent execution failures</h3>
            <ul className="mt-3 space-y-2 text-sm">
              {snapshot.recentErrors.length > 0 ? (
                snapshot.recentErrors.map((record, index) => (
                  <li key={`${record.executed_at}-${index}`} className="rounded border border-red-200 bg-red-50 p-3">
                    <p className="font-semibold text-red-700">
                      <code className="rounded bg-white px-1">{record.command}</code> exited with {record.exit_code}
                    </p>
                    <p className="mt-1 text-xs text-red-700/80">
                      {new Date(record.executed_at).toLocaleString()} · duration {record.duration_ms}ms
                    </p>
                    {record.stderr && <pre className="mt-2 whitespace-pre-wrap text-xs text-red-800">{record.stderr}</pre>}
                  </li>
                ))
              ) : (
                <li className="text-sm text-slate-500">No failures reported in the latest executions.</li>
              )}
            </ul>
          </div>
        </div>
      )}

      {!snapshot && !loading && (
        <p className="text-sm text-slate-500">
          {token
            ? "No telemetry data was returned for this namespace."
            : "Provide an admin or namespace token to query telemetry."}
        </p>
      )}
      {loading && <p className="text-sm text-slate-500">Loading telemetry…</p>}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-slate-200 bg-slate-50 p-3">
      <dt className="text-xs uppercase tracking-wide text-slate-500">{label}</dt>
      <dd className="mt-1 text-lg font-semibold text-slate-900">{value}</dd>
    </div>
  );
}

function extractErrorMessage(error: unknown) {
  if (error instanceof DaemonApiError) {
    return `${error.message} (status ${error.status})`;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unexpected error";
}
