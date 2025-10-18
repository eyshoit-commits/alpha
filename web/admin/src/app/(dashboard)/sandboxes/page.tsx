"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiClient,
  DaemonApiError,
  ExecutionRecord,
  Sandbox,
  sharedApiClient,
} from "@shared/api";
import { useToken } from "@/components/token-context";

interface FormState {
  namespace: string;
  name: string;
  runtime: string;
  cpuMillis?: number;
  memoryMib?: number;
  diskMib?: number;
  timeoutSeconds?: number;
}

export default function SandboxesPage() {
  const { token } = useToken();
  const [form, setForm] = useState<FormState>({ namespace: "default", name: "", runtime: "nodejs" });
  const [sandboxes, setSandboxes] = useState<Sandbox[]>([]);
  const [executions, setExecutions] = useState<Record<string, ExecutionRecord[]>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const client: ApiClient | null = useMemo(() => {
    if (!token) {
      return null;
    }
    return sharedApiClient.withToken(token);
  }, [token]);

  const loadSandboxes = useCallback(async () => {
    if (!client || !form.namespace) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const items = await client.listSandboxes(form.namespace);
      setSandboxes(items);
    } catch (err) {
      setError(extractErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [client, form.namespace]);

  useEffect(() => {
    void loadSandboxes();
  }, [loadSandboxes]);

  const handleCreate = async () => {
    if (!client) {
      setError("Provide a bearer token first.");
      return;
    }
    if (!form.namespace || !form.name) {
      setError("Namespace and name are required.");
      return;
    }

    setError(null);
    try {
      await client.createSandbox({
        namespace: form.namespace,
        name: form.name,
        runtime: form.runtime || undefined,
        limits:
          form.cpuMillis || form.memoryMib || form.diskMib || form.timeoutSeconds
            ? {
                cpu_millis: form.cpuMillis,
                memory_mib: form.memoryMib,
                disk_mib: form.diskMib,
                timeout_seconds: form.timeoutSeconds,
              }
            : undefined,
      });
      setForm((state) => ({ ...state, name: "" }));
      await loadSandboxes();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleStart = async (id: string) => {
    if (!client) return;
    try {
      await client.startSandbox(id);
      await loadSandboxes();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleStop = async (id: string) => {
    if (!client) return;
    try {
      await client.stopSandbox(id);
      await loadSandboxes();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleDelete = async (id: string) => {
    if (!client) return;
    try {
      await client.deleteSandbox(id);
      await loadSandboxes();
      setExecutions((prev) => {
        const clone = { ...prev };
        delete clone[id];
        return clone;
      });
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const loadExecutions = async (id: string) => {
    if (!client) return;
    try {
      const history = await client.listExecutions(id, 10);
      setExecutions((prev) => ({ ...prev, [id]: history }));
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  useEffect(() => {
    if (selectedId) {
      void loadExecutions(selectedId);
    }
  }, [selectedId]);

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h2 className="text-xl font-semibold">Provision a sandbox</h2>
        <p className="mt-2 text-sm text-slate-600">
          Namespace keys are authorized for lifecycle operations. Provide optional limits to override defaults defined in
          <code className="mx-1 rounded bg-slate-100 px-1">config/sandbox_config.toml</code>.
        </p>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <label className="flex flex-col text-sm">
            <span className="font-medium">Namespace</span>
            <input
              value={form.namespace}
              onChange={(event) => setForm((state) => ({ ...state, namespace: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="default"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Sandbox name</span>
            <input
              value={form.name}
              onChange={(event) => setForm((state) => ({ ...state, name: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="build-runner"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Runtime</span>
            <input
              value={form.runtime}
              onChange={(event) => setForm((state) => ({ ...state, runtime: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="nodejs"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">CPU (millis)</span>
            <input
              type="number"
              value={form.cpuMillis ?? ""}
              onChange={(event) => setForm((state) => ({ ...state, cpuMillis: event.target.value ? Number(event.target.value) : undefined }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Memory (MiB)</span>
            <input
              type="number"
              value={form.memoryMib ?? ""}
              onChange={(event) => setForm((state) => ({ ...state, memoryMib: event.target.value ? Number(event.target.value) : undefined }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Disk (MiB)</span>
            <input
              type="number"
              value={form.diskMib ?? ""}
              onChange={(event) => setForm((state) => ({ ...state, diskMib: event.target.value ? Number(event.target.value) : undefined }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Timeout (seconds)</span>
            <input
              type="number"
              value={form.timeoutSeconds ?? ""}
              onChange={(event) =>
                setForm((state) => ({ ...state, timeoutSeconds: event.target.value ? Number(event.target.value) : undefined }))
              }
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
        </div>
        <div className="mt-4 flex gap-3">
          <button
            onClick={handleCreate}
            className="rounded-md bg-emerald-600 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-emerald-500"
          >
            Create sandbox
          </button>
          <button
            onClick={loadSandboxes}
            className="rounded-md border border-slate-200 bg-white px-4 py-2 text-sm shadow-sm"
          >
            Refresh
          </button>
        </div>
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
      </div>

      <div className="rounded-lg border border-slate-200 bg-white shadow-sm">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
          <h2 className="text-lg font-semibold">Sandboxes in namespace: {form.namespace || "–"}</h2>
          {loading && <span className="text-sm text-slate-500">Loading…</span>}
        </div>
        <div className="overflow-x-auto">
          <table className="min-w-full divide-y divide-slate-200 text-sm">
            <thead className="bg-slate-50">
              <tr>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Name</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Runtime</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Status</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Limits</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Updated</th>
                <th className="px-4 py-2 text-right font-medium text-slate-600">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {sandboxes.map((sandbox) => (
                <tr key={sandbox.id} className={selectedId === sandbox.id ? "bg-slate-100" : undefined}>
                  <td className="px-4 py-2">
                    <button
                      className="text-left font-medium text-slate-800 hover:underline"
                      onClick={() => setSelectedId((current) => (current === sandbox.id ? null : sandbox.id))}
                    >
                      {sandbox.name}
                    </button>
                  </td>
                  <td className="px-4 py-2 text-slate-600">{sandbox.runtime}</td>
                  <td className="px-4 py-2">
                    <StatusBadge status={sandbox.status} />
                  </td>
                  <td className="px-4 py-2 text-slate-600">
                    {sandbox.limits.cpu_millis} cpu · {sandbox.limits.memory_mib} MiB · {sandbox.limits.disk_mib} MiB
                  </td>
                  <td className="px-4 py-2 text-slate-600">
                    {new Date(sandbox.updated_at).toLocaleString()}
                  </td>
                  <td className="px-4 py-2">
                    <div className="flex justify-end gap-2">
                      <button
                        onClick={() => handleStart(sandbox.id)}
                        className="rounded-md border border-emerald-200 bg-emerald-50 px-3 py-1 text-xs font-semibold text-emerald-700"
                      >
                        Start
                      </button>
                      <button
                        onClick={() => handleStop(sandbox.id)}
                        className="rounded-md border border-amber-200 bg-amber-50 px-3 py-1 text-xs font-semibold text-amber-700"
                      >
                        Stop
                      </button>
                      <button
                        onClick={() => handleDelete(sandbox.id)}
                        className="rounded-md border border-red-200 bg-red-50 px-3 py-1 text-xs font-semibold text-red-700"
                      >
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
              {sandboxes.length === 0 && (
                <tr>
                  <td className="px-4 py-6 text-center text-slate-500" colSpan={6}>
                    {token
                      ? "No sandboxes were returned for this namespace."
                      : "Provide a namespace-scoped token to list sandboxes."}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {selectedId && (
        <ExecutionDrawer
          sandbox={sandboxes.find((item) => item.id === selectedId) ?? null}
          executions={executions[selectedId] ?? []}
          onRefresh={() => void loadExecutions(selectedId)}
        />
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

function StatusBadge({ status }: { status: string }) {
  const palette: Record<string, string> = {
    running: "bg-emerald-100 text-emerald-700 border border-emerald-200",
    stopped: "bg-slate-100 text-slate-700 border border-slate-200",
    pending: "bg-amber-100 text-amber-700 border border-amber-200",
    failed: "bg-red-100 text-red-700 border border-red-200",
  };
  const normalized = status.toLowerCase();
  const className = palette[normalized] ?? "bg-slate-100 text-slate-700 border border-slate-200";
  return <span className={`rounded-full px-3 py-1 text-xs font-semibold capitalize ${className}`}>{status}</span>;
}

function ExecutionDrawer({
  sandbox,
  executions,
  onRefresh,
}: {
  sandbox: Sandbox | null;
  executions: ExecutionRecord[];
  onRefresh: () => void;
}) {
  if (!sandbox) return null;
  return (
    <aside className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-lg font-semibold">Recent executions · {sandbox.name}</h3>
          <p className="text-sm text-slate-600">
            Track command history to audit sandbox interactions and latency trends.
          </p>
        </div>
        <button
          onClick={onRefresh}
          className="rounded-md border border-slate-200 bg-white px-3 py-2 text-sm shadow-sm"
        >
          Refresh history
        </button>
      </div>
      <div className="mt-4 overflow-x-auto">
        <table className="min-w-full divide-y divide-slate-200 text-sm">
          <thead>
            <tr className="bg-slate-50">
              <th className="px-3 py-2 text-left font-medium text-slate-600">Command</th>
              <th className="px-3 py-2 text-left font-medium text-slate-600">Executed</th>
              <th className="px-3 py-2 text-left font-medium text-slate-600">Exit</th>
              <th className="px-3 py-2 text-left font-medium text-slate-600">Duration (ms)</th>
              <th className="px-3 py-2 text-left font-medium text-slate-600">Timed out</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {executions.map((execution, index) => (
              <tr key={`${execution.executed_at}-${index}`}>
                <td className="px-3 py-2 font-medium text-slate-800">
                  <code className="rounded bg-slate-100 px-1">{execution.command}</code>
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
            {executions.length === 0 && (
              <tr>
                <td colSpan={5} className="px-3 py-6 text-center text-slate-500">
                  No executions recorded yet.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </aside>
  );
}
