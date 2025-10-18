"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiClient,
  DaemonApiError,
  IssuedKeyResponse,
  KeyInfo,
  KeyScope,
  RotatedKeyResponse,
  sharedApiClient,
} from "@shared/api";
import { useToken } from "@/components/token-context";
import { Button, Input, Select, Alert, Card } from "@/components/ui";

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
  const [form, setForm] = useState<KeyFormState>({ type: "namespace", namespace: "default", rateLimit: 100, ttlHours: 24 });
  const [issuing, setIssuing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [issued, setIssued] = useState<IssuedKeyResponse | null>(null);
  const [rotationResult, setRotationResult] = useState<RotatedKeyResponse | null>(null);
  const [rotationAck, setRotationAck] = useState<string | null>(null);
  const [rotateKeyId, setRotateKeyId] = useState<string>("");
  const [rotateRateLimit, setRotateRateLimit] = useState<number | "">("");
  const [rotateTtlHours, setRotateTtlHours] = useState<number | "">("");

  const rateLimitOptions = [
    { value: "50", label: "50 req/min (Low)" },
    { value: "100", label: "100 req/min (Standard)" },
    { value: "200", label: "200 req/min (High)" },
    { value: "500", label: "500 req/min (Premium)" },
    { value: "1000", label: "1000 req/min (Enterprise)" },
  ];

  const ttlOptions = [
    { value: "1", label: "1 hour" },
    { value: "24", label: "1 day" },
    { value: "168", label: "1 week" },
    { value: "720", label: "30 days" },
    { value: "8760", label: "1 year" },
    { value: "", label: "Never expires" },
  ];

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
    setRotationResult(null);
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

  const handleRotate = async () => {
    if (!client) {
      setError("Provide an admin token before rotating keys.");
      return;
    }
    if (!rotateKeyId) {
      setError("Select a key to rotate.");
      return;
    }

    const payload: { key_id: string; rate_limit?: number; ttl_seconds?: number } = { key_id: rotateKeyId };
    if (rotateRateLimit !== "" && rotateRateLimit > 0) {
      payload.rate_limit = rotateRateLimit;
    }
    if (rotateTtlHours !== "" && rotateTtlHours > 0) {
      payload.ttl_seconds = rotateTtlHours * 3600;
    }

    setError(null);
    setRotationAck(null);
    setRotationResult(null);
    try {
      const response = await client.rotateKey(payload);
      setRotationResult(response);
      await loadKeys();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleAcknowledge = async () => {
    if (!client || !rotationResult) return;
    try {
      await client.acknowledgeRotation(rotationResult.webhook.payload, rotationResult.webhook.signature);
      setRotationAck("Rotation webhook acknowledged");
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  return (
    <section className="space-y-8">
      <Card
        title="Issue a new API key"
        description="Admin keys can manage global resources while namespace keys are restricted to lifecycle operations. Keys are issued once and only the prefix is persisted server-side."
      >
        <div className="grid gap-6 md:grid-cols-2">
          <Select
            label="Scope"
            value={form.type}
            onChange={(value) => setForm((state) => ({ ...state, type: value as KeyFormState["type"] }))}
            options={[
              { value: "namespace", label: "🔷 Namespace Scoped" },
              { value: "admin", label: "👑 Admin (Full Access)" },
            ]}
          />
          {form.type === "namespace" && (
            <Input
              label="Namespace"
              value={form.namespace}
              onChange={(e) => setForm((state) => ({ ...state, namespace: e.target.value }))}
              placeholder="e.g., default"
            />
          )}
          <Select
            label="Rate Limit"
            value={String(form.rateLimit)}
            onChange={(value) => setForm((state) => ({ ...state, rateLimit: Number(value) }))}
            options={rateLimitOptions}
          />
          <Select
            label="Expiration"
            value={String(form.ttlHours)}
            onChange={(value) => setForm((state) => ({ ...state, ttlHours: value === "" ? "" : Number(value) }))}
            options={ttlOptions}
          />
        </div>
        <div className="mt-6 flex gap-3">
          <Button onClick={handleIssue} loading={issuing}>
            {issuing ? "Creating..." : "🔑 Issue Key"}
          </Button>
          <Button variant="secondary" onClick={loadKeys}>
            🔄 Refresh
          </Button>
        </div>
        {error && (
          <Alert variant="error" title="Error" onClose={() => setError(null)}>
            {error}
          </Alert>
        )}
        {issued && (
          <Alert variant="success" title="✨ New Token Generated">
            <p className="font-mono break-all bg-white p-3 rounded border-2 border-blue-200 mt-2">
              {issued.token}
            </p>
            <p className="mt-3 font-semibold">⚠️ Copy this token now - it won't be shown again!</p>
          </Alert>
        )}
        {rotationResult && (
          <Alert variant="info" title="🔄 Rotation Completed">
            <p className="text-sm text-slate-600 mb-2">
              Previous key <code className="bg-blue-100 px-2 py-1 rounded font-mono">{rotationResult.previous.key_prefix}</code> superseded
            </p>
            <p className="font-mono break-all bg-white p-3 rounded border-2 border-blue-200 mt-2">
              {rotationResult.token}
            </p>
            <p className="text-sm mt-3">
              Webhook event <code className="bg-blue-100 px-2 py-1 rounded font-mono">{rotationResult.webhook.event_id}</code> pending acknowledgement
            </p>
            <div className="flex gap-2 mt-4">
              <Button size="sm" onClick={handleAcknowledge}>
                Acknowledge Webhook
              </Button>
              {rotationAck && <span className="self-center text-sm font-medium text-blue-700">{rotationAck}</span>}
            </div>
          </Alert>
        )}
      </Card>

      <Card
        title="Rotate an existing key"
        description="Rotations mint a replacement token while preserving audit history. Configure updated limits or TTL to enforce new policies."
      >
        <div className="grid gap-6 md:grid-cols-3">
          <div className="md:col-span-3">
            <Select
              label="Key to rotate"
              value={rotateKeyId}
              onChange={setRotateKeyId}
              options={[
                { value: "", label: "Select a key to rotate..." },
                ...keys.map((key) => ({
                  value: key.id,
                  label: `${key.key_prefix} · ${renderScope(key.scope)}`,
                })),
              ]}
            />
          </div>
          <Input
            type="number"
            label="New Rate Limit (optional)"
            value={rotateRateLimit === "" ? "" : String(rotateRateLimit)}
            onChange={(e) => setRotateRateLimit(e.target.value === "" ? "" : Number(e.target.value))}
            placeholder="Keep existing rate limit"
          />
          <Input
            type="number"
            label="New TTL in hours (optional)"
            value={rotateTtlHours === "" ? "" : String(rotateTtlHours)}
            onChange={(e) => setRotateTtlHours(e.target.value === "" ? "" : Number(e.target.value))}
            placeholder="Keep existing TTL"
          />
        </div>
        <div className="mt-6 flex gap-3">
          <Button onClick={handleRotate}>🔄 Rotate Key</Button>
          <Button
            variant="ghost"
            onClick={() => {
              setRotateKeyId("");
              setRotateRateLimit("");
              setRotateTtlHours("");
            }}
          >
            ✕ Clear
          </Button>
        </div>
      </Card>

      <Card
        title="Issued Keys"
        actions={<span className="text-base text-slate-600 font-semibold">Total: {keys.length}</span>}
      >
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
      </Card>
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
