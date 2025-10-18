"use client";

import { useEffect, useMemo, useState } from "react";
import { ApiClient, DaemonApiError, ExecutionRecord, Sandbox, sharedApiClient } from "@shared/api";
import { useToken } from "@/components/token-context";

export default function HistoryPage() {
  const { token } = useToken();
  const client: ApiClient | null = useMemo(() => (token ? sharedApiClient.withToken(token) : null), [token]);
  const [namespace, setNamespace] = useState("default");
  const [sandboxes, setSandboxes] = useState<Sandbox[]>([]);
  const [history, setHistory] = useState<Record<string, ExecutionRecord[]>>({});
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const load = async () => {
      if (!client || !namespace) return;
      setLoading(true);
      setError(null);
      try {
        const items = await client.listSandboxes(namespace);
        setSandboxes(items);
        const executionData = await Promise.all(items.map((item) => client.listExecutions(item.id, 10)));
        const next: Record<string, ExecutionRecord[]> = {};
        items.forEach((item, index) => {
          next[item.id] = executionData[index];
        });
        setHistory(next);
      } catch (err) {
        setError(extractErrorMessage(err));
      } finally {
        setLoading(false);
      }
    };
    void load();
  }, [client, namespace]);

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h2 className="text-xl font-semibold">Execution history</h2>
        <p className="mt-2 text-sm text-slate-600">
          Review command runs across sandboxes to audit behaviour and ensure successful automation.
        </p>
        <label className="mt-4 flex max-w-xs flex-col text-sm">
          <span className="font-medium">Namespace</span>
          <input
            value={namespace}
            onChange={(event) => setNamespace(event.target.value)}
            className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
          />
        </label>
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
        {loading && <p className="mt-3 text-sm text-slate-500">Loading…</p>}
      </div>

      {sandboxes.map((sandbox) => (
        <article key={sandbox.id} className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div>
              <h3 className="text-lg font-semibold">{sandbox.name}</h3>
              <p className="text-sm text-slate-500">Runtime {sandbox.runtime} · Status {sandbox.status}</p>
            </div>
            <span className="text-xs uppercase tracking-wide text-slate-500">
              Updated {new Date(sandbox.updated_at).toLocaleString()}
            </span>
          </header>
          <table className="mt-4 min-w-full divide-y divide-slate-200 text-sm">
            <thead className="bg-slate-50">
              <tr>
                <th className="px-3 py-2 text-left font-medium text-slate-600">Command</th>
                <th className="px-3 py-2 text-left font-medium text-slate-600">Executed</th>
                <th className="px-3 py-2 text-left font-medium text-slate-600">Exit</th>
                <th className="px-3 py-2 text-left font-medium text-slate-600">Duration (ms)</th>
                <th className="px-3 py-2 text-left font-medium text-slate-600">Timed out</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {(history[sandbox.id] ?? []).map((execution, index) => (
                <tr key={`${execution.executed_at}-${index}`}>
                  <td className="px-3 py-2">
                    <code className="rounded bg-slate-100 px-1 text-xs">{execution.command}</code>
                    {execution.args.length > 0 && (
                      <span className="ml-2 text-slate-600">{execution.args.join(" ")}</span>
                    )}
                  </td>
                  <td className="px-3 py-2 text-slate-600">
                    {new Date(execution.executed_at).toLocaleString()}
                  </td>
                  <td className="px-3 py-2 text-slate-600">{execution.exit_code ?? "—"}</td>
                  <td className="px-3 py-2 text-slate-600">{execution.duration_ms}</td>
                  <td className="px-3 py-2 text-slate-600">{execution.timed_out ? "Yes" : "No"}</td>
                </tr>
              ))}
              {(history[sandbox.id] ?? []).length === 0 && (
                <tr>
                  <td colSpan={5} className="px-3 py-4 text-center text-slate-500">
                    No executions recorded yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </article>
      ))}

      {sandboxes.length === 0 && !loading && (
        <p className="text-sm text-slate-500">
          {token ? "No sandboxes found for this namespace." : "Provide a namespace token to fetch execution history."}
        </p>
      )}
    </section>
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
