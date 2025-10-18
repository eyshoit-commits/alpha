"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { ApiClient, AuditEvent, DaemonApiError, ListAuditEventsParams, sharedApiClient } from "@shared/api";
import { useToken } from "@/components/token-context";

interface FilterState {
  namespace: string;
  eventType: string;
  limit: number;
  since?: string;
  until?: string;
}

export default function AuditPage() {
  const { token } = useToken();
  const client: ApiClient | null = useMemo(() => {
    if (!token) return null;
    return sharedApiClient.withToken(token);
  }, [token]);

  const [filters, setFilters] = useState<FilterState>({ namespace: "", eventType: "", limit: 50 });
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadEvents = useCallback(async () => {
    if (!client) {
      return;
    }
    setLoading(true);
    setError(null);
    const params: ListAuditEventsParams = {};
    if (filters.namespace.trim()) {
      params.namespace = filters.namespace.trim();
    }
    if (filters.eventType.trim()) {
      params.eventType = filters.eventType.trim();
    }
    if (filters.limit > 0) {
      params.limit = filters.limit;
    }
    if (filters.since) {
      params.since = filters.since;
    }
    if (filters.until) {
      params.until = filters.until;
    }
    try {
      const response = await client.listAuditEvents(params);
      setEvents(response);
    } catch (err) {
      setError(extractErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [client, filters]);

  useEffect(() => {
    void loadEvents();
  }, [loadEvents]);

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <header className="space-y-1">
          <h2 className="text-xl font-semibold">Audit log explorer</h2>
          <p className="text-sm text-slate-600">
            Review cryptographically signed audit entries emitted by the daemon. The explorer lets you filter by namespace,
            event type, and time range to support incident response workflows.
          </p>
        </header>
        <div className="mt-4 grid gap-4 md:grid-cols-3">
          <label className="flex flex-col text-sm">
            <span className="font-medium">Namespace</span>
            <input
              value={filters.namespace}
              onChange={(event) => setFilters((state) => ({ ...state, namespace: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="default"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Event type</span>
            <input
              value={filters.eventType}
              onChange={(event) => setFilters((state) => ({ ...state, eventType: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="sandbox_exec"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Limit</span>
            <input
              type="number"
              min={1}
              max={200}
              value={filters.limit}
              onChange={(event) => setFilters((state) => ({ ...state, limit: Number(event.target.value) }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Since</span>
            <input
              type="datetime-local"
              value={filters.since ?? ""}
              onChange={(event) => setFilters((state) => ({ ...state, since: event.target.value || undefined }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Until</span>
            <input
              type="datetime-local"
              value={filters.until ?? ""}
              onChange={(event) => setFilters((state) => ({ ...state, until: event.target.value || undefined }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => setFilters({ namespace: "", eventType: "", limit: 50 })}
            className="rounded-md border border-slate-200 bg-white px-4 py-2 text-sm shadow-sm"
          >
            Reset
          </button>
          <button
            type="button"
            onClick={() => void loadEvents()}
            className="rounded-md bg-slate-900 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-slate-700"
          >
            Apply filters
          </button>
        </div>
      </div>

      {error ? <p className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p> : null}

      <section className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <header className="flex items-center justify-between">
          <div>
            <h3 className="text-lg font-semibold">Audit events</h3>
            <p className="text-xs uppercase tracking-wide text-slate-500">
              {loading ? "Loading…" : `${events.length} result${events.length === 1 ? "" : "s"}`}
            </p>
          </div>
          <button
            type="button"
            className="rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-semibold uppercase tracking-wide"
            onClick={() => void loadEvents()}
          >
            Refresh
          </button>
        </header>
        <div className="mt-4 space-y-4">
          {events.length === 0 ? (
            <p className="text-sm text-slate-500">No audit events for the selected filters.</p>
          ) : (
            events.map((event) => (
              <article key={event.id} className="space-y-2 rounded-md border border-slate-200 bg-slate-50 p-4 shadow-sm">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <p className="text-sm font-semibold text-slate-900">{event.event_type}</p>
                    <p className="text-xs text-slate-500">
                      Recorded {new Date(event.recorded_at).toLocaleString()} · Namespace {event.namespace || "–"}
                    </p>
                  </div>
                  <span
                    className={`rounded-full px-3 py-1 text-xs font-medium ${
                      event.signature_valid === false
                        ? "bg-red-100 text-red-700"
                        : event.signature_valid === true
                        ? "bg-emerald-100 text-emerald-700"
                        : "bg-slate-200 text-slate-700"
                    }`}
                  >
                    {event.signature_valid === false
                      ? "Signature invalid"
                      : event.signature_valid === true
                      ? "Signature valid"
                      : "Unsigned"}
                  </span>
                </div>
                {event.actor ? <p className="text-xs text-slate-500">Actor: {event.actor}</p> : null}
                <pre className="overflow-x-auto rounded-md bg-slate-900 p-4 text-xs text-slate-100">
                  {JSON.stringify(event.payload, null, 2)}
                </pre>
              </article>
            ))
          )}
        </div>
      </section>
    </section>
  );
}

function extractErrorMessage(error: unknown) {
  if (!error) return "Unexpected error";
  if (error instanceof DaemonApiError) {
    const body = error.body as { error?: string } | undefined;
    if (body?.error) return body.error;
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unexpected error";
}
