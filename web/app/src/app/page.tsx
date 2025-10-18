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

export default function NamespaceDashboard() {
  const { token } = useToken();
  const client: ApiClient | null = useMemo(() => (token ? sharedApiClient.withToken(token) : null), [token]);
  const [namespace, setNamespace] = useState("default");
  const [sandboxes, setSandboxes] = useState<Sandbox[]>([]);
  const [selectedSandbox, setSelectedSandbox] = useState<string | null>(null);
  const [command, setCommand] = useState("/bin/true");
  const [args, setArgs] = useState("");
  const [logs, setLogs] = useState<ExecutionRecord | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [createName, setCreateName] = useState("");

  const loadSandboxes = useCallback(async () => {
    if (!client || !namespace) {
      return;
    }
    setError(null);
    try {
      const data = await client.listSandboxes(namespace);
      setSandboxes(data);
      if (data.length > 0 && !selectedSandbox) {
        setSelectedSandbox(data[0].id);
      }
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  }, [client, namespace, selectedSandbox]);

  useEffect(() => {
    void loadSandboxes();
  }, [loadSandboxes]);

  const handleCreate = async () => {
    if (!client) {
      setError("Provide a namespace token to create sandboxes.");
      return;
    }
    if (!createName) {
      setError("Choose a sandbox name first.");
      return;
    }
    setError(null);
    try {
      const sandbox = await client.createSandbox({ namespace, name: createName });
      setSandboxes((current) => [...current, sandbox]);
      setCreateName("");
      setSelectedSandbox(sandbox.id);
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleStart = async () => {
    if (!client || !selectedSandbox) return;
    try {
      await client.startSandbox(selectedSandbox);
      setStatus("Sandbox started successfully.");
      await loadSandboxes();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleStop = async () => {
    if (!client || !selectedSandbox) return;
    try {
      await client.stopSandbox(selectedSandbox);
      setStatus("Sandbox stopped.");
      await loadSandboxes();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleDelete = async () => {
    if (!client || !selectedSandbox) return;
    try {
      await client.deleteSandbox(selectedSandbox);
      setSandboxes((current) => current.filter((item) => item.id !== selectedSandbox));
      setSelectedSandbox(null);
      setLogs(null);
      setStatus("Sandbox deleted.");
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleExec = async () => {
    if (!client || !selectedSandbox) return;
    setStatus(null);
    setError(null);
    try {
      const execution = await client.exec(selectedSandbox, {
        command,
        args: args.trim() ? args.split(/\s+/) : undefined,
      });
      setLogs(execution);
      setStatus("Command executed.");
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  useEffect(() => {
    const fetchLastExecution = async () => {
      if (!client || !selectedSandbox) return;
      try {
        const history = await client.listExecutions(selectedSandbox, 1);
        setLogs(history[0] ?? null);
      } catch (err) {
        // Swallow errors here to avoid clobbering other operations.
      }
    };
    void fetchLastExecution();
  }, [client, selectedSandbox]);

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h2 className="text-xl font-semibold">Sandbox lifecycle</h2>
        <p className="mt-2 text-sm text-slate-600">
          Create and manage sandboxes within your namespace. All operations call the daemon&rsquo;s lifecycle API directly with the
          bearer token provided above.
        </p>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <label className="flex flex-col text-sm">
            <span className="font-medium">Namespace</span>
            <input
              value={namespace}
              onChange={(event) => {
                setNamespace(event.target.value);
                setSelectedSandbox(null);
              }}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="default"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">New sandbox name</span>
            <input
              value={createName}
              onChange={(event) => setCreateName(event.target.value)}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="ci-runner"
            />
          </label>
        </div>
        <div className="mt-4 flex flex-wrap gap-3 text-sm">
          <button
            onClick={handleCreate}
            className="rounded-md bg-emerald-600 px-4 py-2 font-semibold text-white shadow-sm hover:bg-emerald-500"
          >
            Create
          </button>
          <button
            onClick={loadSandboxes}
            className="rounded-md border border-slate-200 bg-white px-4 py-2 shadow-sm"
          >
            Refresh list
          </button>
        </div>
        {status && <p className="mt-3 text-sm text-emerald-700">{status}</p>}
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
      </div>

      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h3 className="text-lg font-semibold">Sandboxes</h3>
        <ul className="mt-3 space-y-2 text-sm">
          {sandboxes.map((sandbox) => (
            <li key={sandbox.id}>
              <button
                className={`flex w-full items-center justify-between rounded-md border px-4 py-2 text-left shadow-sm ${
                  selectedSandbox === sandbox.id
                    ? "border-slate-300 bg-slate-100"
                    : "border-slate-200 bg-white hover:border-slate-300"
                }`}
                onClick={() => setSelectedSandbox(sandbox.id)}
              >
                <span>
                  <span className="font-semibold">{sandbox.name}</span>
                  <span className="ml-2 text-slate-500">· {sandbox.runtime}</span>
                </span>
                <span className="text-xs uppercase tracking-wide text-slate-500">{sandbox.status}</span>
              </button>
            </li>
          ))}
          {sandboxes.length === 0 && (
            <li className="rounded-md border border-dashed border-slate-300 bg-slate-50 px-4 py-4 text-center text-slate-500">
              {token ? "No sandboxes available yet." : "Add a namespace token to load sandboxes."}
            </li>
          )}
        </ul>
        {selectedSandbox && (
          <div className="mt-4 flex flex-wrap gap-3 text-sm">
            <button
              onClick={handleStart}
              className="rounded-md border border-emerald-200 bg-emerald-50 px-4 py-2 font-semibold text-emerald-700"
            >
              Start
            </button>
            <button
              onClick={handleStop}
              className="rounded-md border border-amber-200 bg-amber-50 px-4 py-2 font-semibold text-amber-700"
            >
              Stop
            </button>
            <button
              onClick={handleDelete}
              className="rounded-md border border-red-200 bg-red-50 px-4 py-2 font-semibold text-red-700"
            >
              Delete
            </button>
          </div>
        )}
      </div>

      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h3 className="text-lg font-semibold">Execute command</h3>
        <p className="mt-2 text-sm text-slate-600">
          Send a command to the selected sandbox. Execution output and metadata will appear below.
        </p>
        <div className="mt-4 grid gap-4 md:grid-cols-2 text-sm">
          <label className="flex flex-col">
            <span className="font-medium">Command</span>
            <input
              value={command}
              onChange={(event) => setCommand(event.target.value)}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col">
            <span className="font-medium">Arguments</span>
            <input
              value={args}
              onChange={(event) => setArgs(event.target.value)}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="--version"
            />
          </label>
        </div>
        <button
          onClick={handleExec}
          className="mt-4 rounded-md bg-slate-900 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-slate-700"
          disabled={!selectedSandbox}
        >
          Run command
        </button>
        {logs && (
          <div className="mt-4 space-y-2 rounded-md border border-slate-200 bg-slate-50 p-4 text-xs">
            <p>
              <span className="font-semibold">Exit code:</span> {logs.exit_code ?? "n/a"}
            </p>
            <p>
              <span className="font-semibold">Duration:</span> {logs.duration_ms} ms
            </p>
            {logs.stdout && (
              <div>
                <span className="font-semibold">stdout</span>
                <pre className="mt-1 whitespace-pre-wrap">{logs.stdout}</pre>
              </div>
            )}
            {logs.stderr && (
              <div>
                <span className="font-semibold">stderr</span>
                <pre className="mt-1 whitespace-pre-wrap">{logs.stderr}</pre>
              </div>
            )}
          </div>
        )}
      </div>

      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h3 className="text-lg font-semibold">Documentation</h3>
        <p className="mt-2 text-sm text-slate-600">
          These resources cover daemon APIs, sandbox limits, and operational runbooks available in the repository.
        </p>
        <ul className="mt-3 list-disc space-y-2 pl-5 text-sm text-slate-700">
          <li>
            <a href="https://github.com/alpha/alpha/blob/main/docs/api.md" target="_blank" rel="noreferrer">
              API reference
            </a>
          </li>
          <li>
            <a href="https://github.com/alpha/alpha/blob/main/docs/operations.md" target="_blank" rel="noreferrer">
              Operations handbook
            </a>
          </li>
          <li>
            <a href="https://github.com/alpha/alpha/blob/main/docs/testing.md" target="_blank" rel="noreferrer">
              Testing strategy
            </a>
          </li>
        </ul>
      </div>
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
