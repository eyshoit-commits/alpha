"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { ApiClient, DaemonApiError, IssuedKeyResponse, KeyInfo, KeyScope, sharedApiClient } from "@shared/api";
import { useToken } from "@/components/token-context";

interface KeyFormState {
  type: "admin" | "namespace";
  namespace: string;
  rateLimit: number;
  ttlHours: number | "";
}

export default function KeysPage() {
  const { token } = useToken();
  const client: ApiClient | null = useMemo(() => (token ? sharedApiClient.withToken(token) : null), [token]);
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [form, setForm] = useState<KeyFormState>({ type: "namespace", namespace: "default", rateLimit: 100, ttlHours: "" });
  const [issuing, setIssuing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [issued, setIssued] = useState<IssuedKeyResponse | null>(null);

  const loadKeys = useCallback(async () => {
    if (!client) return;
    try {
      const result = await client.listKeys();
      setKeys(result);
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  }, [client]);

  useEffect(() => {
    void loadKeys();
  }, [loadKeys]);

  const handleIssue = async () => {
    if (!client) {
      setError("Provide an admin-scoped token first.");
      return;
    }

    const scope: KeyScope =
      form.type === "admin"
        ? { type: "admin" }
        : {
            type: "namespace",
            namespace: form.namespace,
          };

    const ttlSeconds = form.ttlHours === "" ? undefined : Number(form.ttlHours) * 3600;

    setIssuing(true);
    setError(null);
    setIssued(null);
    try {
      const response = await client.issueKey({
        scope,
        rate_limit: form.rateLimit,
        ttl_seconds: ttlSeconds,
      });
      setIssued(response);
      await loadKeys();
    } catch (err) {
      setError(extractErrorMessage(err));
    } finally {
      setIssuing(false);
    }
  };

  const handleRevoke = async (id: string) => {
    if (!client) return;
    setError(null);
    try {
      await client.revokeKey(id);
      await loadKeys();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <h2 className="text-xl font-semibold">Issue a new API key</h2>
        <p className="mt-2 text-sm text-slate-600">
          Admin keys can manage global resources while namespace keys are restricted to lifecycle operations for their
          namespace. Keys are issued once and only the prefix is persisted server-side.
        </p>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <label className="flex flex-col text-sm">
            <span className="font-medium">Scope</span>
            <select
              value={form.type}
              onChange={(event) => setForm((state) => ({ ...state, type: event.target.value as KeyFormState["type"] }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            >
              <option value="namespace">Namespace</option>
              <option value="admin">Admin</option>
            </select>
          </label>
          {form.type === "namespace" && (
            <label className="flex flex-col text-sm">
              <span className="font-medium">Namespace</span>
              <input
                value={form.namespace}
                onChange={(event) => setForm((state) => ({ ...state, namespace: event.target.value }))}
                className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              />
            </label>
          )}
          <label className="flex flex-col text-sm">
            <span className="font-medium">Rate limit (req/min)</span>
            <input
              type="number"
              value={form.rateLimit}
              onChange={(event) => setForm((state) => ({ ...state, rateLimit: Number(event.target.value) }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">TTL (hours)</span>
            <input
              type="number"
              value={form.ttlHours}
              onChange={(event) =>
                setForm((state) => ({ ...state, ttlHours: event.target.value === "" ? "" : Number(event.target.value) }))
              }
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="Leave blank for no expiry"
            />
          </label>
        </div>
        <div className="mt-4 flex gap-3">
          <button
            disabled={issuing}
            onClick={handleIssue}
            className="rounded-md bg-emerald-600 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {issuing ? "Issuing…" : "Issue key"}
          </button>
          <button
            onClick={loadKeys}
            className="rounded-md border border-slate-200 bg-white px-4 py-2 text-sm shadow-sm"
          >
            Refresh list
          </button>
        </div>
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
        {issued && (
          <div className="mt-4 rounded-md border border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-800">
            <p className="font-semibold">New token generated</p>
            <p className="mt-2 break-all">
              <span className="font-medium">Token:</span> {issued.token}
            </p>
            <p className="mt-1 text-emerald-700">Copy this token now; it will not be shown again.</p>
          </div>
        )}
      </div>

      <div className="rounded-lg border border-slate-200 bg-white shadow-sm">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
          <h2 className="text-lg font-semibold">Issued keys</h2>
          <span className="text-sm text-slate-500">Total {keys.length}</span>
        </div>
        <div className="overflow-x-auto">
          <table className="min-w-full divide-y divide-slate-200 text-sm">
            <thead className="bg-slate-50">
              <tr>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Prefix</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Scope</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Rate limit</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Created</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Last used</th>
                <th className="px-4 py-2 text-left font-medium text-slate-600">Expires</th>
                <th className="px-4 py-2 text-right font-medium text-slate-600">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {keys.map((key) => (
                <tr key={key.id}>
                  <td className="px-4 py-2 font-mono text-slate-800">{key.key_prefix}</td>
                  <td className="px-4 py-2 text-slate-600">{renderScope(key.scope)}</td>
                  <td className="px-4 py-2 text-slate-600">{key.rate_limit}</td>
                  <td className="px-4 py-2 text-slate-600">{new Date(key.created_at).toLocaleString()}</td>
                  <td className="px-4 py-2 text-slate-600">{key.last_used_at ? new Date(key.last_used_at).toLocaleString() : "—"}</td>
                  <td className="px-4 py-2 text-slate-600">{key.expires_at ? new Date(key.expires_at).toLocaleString() : "—"}</td>
                  <td className="px-4 py-2 text-right">
                    <button
                      onClick={() => handleRevoke(key.id)}
                      className="rounded-md border border-red-200 bg-red-50 px-3 py-1 text-xs font-semibold text-red-700"
                    >
                      Revoke
                    </button>
                  </td>
                </tr>
              ))}
              {keys.length === 0 && (
                <tr>
                  <td className="px-4 py-6 text-center text-slate-500" colSpan={7}>
                    {token ? "No keys issued yet." : "Provide an admin token to load API keys."}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function renderScope(scope: KeyScope) {
  if (scope.type === "admin") {
    return "Admin";
  }
  return `Namespace · ${scope.namespace}`;
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
