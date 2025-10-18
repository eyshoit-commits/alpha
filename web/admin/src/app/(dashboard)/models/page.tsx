"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ApiClient,
  DaemonApiError,
  ModelDownloadJob,
  ModelRecord,
  RegisterModelPayload,
  sharedApiClient,
} from "@shared/api";
import { useToken } from "@/components/token-context";

interface FormState {
  name: string;
  provider: string;
  version: string;
  format: string;
  sourceUri: string;
  checksum?: string;
  sizeBytes?: number;
  tags: string;
}

export default function ModelsPage() {
  const { token } = useToken();
  const client: ApiClient | null = useMemo(() => {
    if (!token) return null;
    return sharedApiClient.withToken(token);
  }, [token]);

  const [models, setModels] = useState<ModelRecord[]>([]);
  const [selectedModel, setSelectedModel] = useState<ModelRecord | null>(null);
  const [jobs, setJobs] = useState<ModelDownloadJob[]>([]);
  const [loading, setLoading] = useState(false);
  const [jobsLoading, setJobsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>({
    name: "",
    provider: "huggingface",
    version: "latest",
    format: "gguf",
    sourceUri: "",
    tags: "",
  });

  const loadModels = useCallback(async () => {
    if (!client) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const response = await client.listModels();
      setModels(response);
      if (selectedModel) {
        const next = response.find((entry) => entry.id === selectedModel.id) ?? null;
        setSelectedModel(next);
      }
    } catch (err) {
      setError(extractErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [client, selectedModel]);

  useEffect(() => {
    void loadModels();
  }, [loadModels]);

  const loadJobs = useCallback(
    async (modelId: string) => {
      if (!client) return;
      setJobsLoading(true);
      try {
        const items = await client.listModelJobs(modelId);
        setJobs(items);
      } catch (err) {
        setError(extractErrorMessage(err));
      } finally {
        setJobsLoading(false);
      }
    },
    [client],
  );

  useEffect(() => {
    if (selectedModel) {
      void loadJobs(selectedModel.id);
    } else {
      setJobs([]);
    }
  }, [selectedModel, loadJobs]);

  const handleRegister = async () => {
    if (!client) {
      setError("Provide an admin token to register models.");
      return;
    }
    if (!form.name || !form.sourceUri) {
      setError("Name and source URI are required.");
      return;
    }

    const payload: RegisterModelPayload = {
      name: form.name,
      provider: form.provider,
      version: form.version,
      format: form.format,
      source_uri: form.sourceUri,
    };

    if (form.checksum) {
      payload.checksum_sha256 = form.checksum;
    }
    if (form.sizeBytes && form.sizeBytes > 0) {
      payload.size_bytes = form.sizeBytes;
    }
    if (form.tags.trim()) {
      payload.tags = form.tags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean);
    }

    setError(null);
    try {
      const record = await client.registerModel(payload);
      setForm((state) => ({ ...state, name: "", sourceUri: "", checksum: undefined, sizeBytes: undefined }));
      await loadModels();
      setSelectedModel(record);
      await loadJobs(record.id);
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleRefresh = async (model: ModelRecord) => {
    if (!client) return;
    try {
      const refreshed = await client.refreshModel(model.id);
      await loadModels();
      setSelectedModel(refreshed);
      await loadJobs(model.id);
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  const handleDelete = async (model: ModelRecord) => {
    if (!client) return;
    try {
      await client.deleteModel(model.id);
      setSelectedModel(null);
      await loadModels();
    } catch (err) {
      setError(extractErrorMessage(err));
    }
  };

  return (
    <section className="space-y-6">
      <div className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
        <header className="space-y-1">
          <h2 className="text-xl font-semibold">Register model artifacts</h2>
          <p className="text-sm text-slate-600">
            Provide the source URI for the artifact (e.g. HuggingFace or internal object storage). The daemon downloads the
            artifact in an isolated sandbox, verifies checksums, and signs the resulting audit trail.
          </p>
        </header>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <label className="flex flex-col text-sm">
            <span className="font-medium">Model name</span>
            <input
              value={form.name}
              onChange={(event) => setForm((state) => ({ ...state, name: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="phi-3-mini"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Provider</span>
            <input
              value={form.provider}
              onChange={(event) => setForm((state) => ({ ...state, provider: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="huggingface"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Version</span>
            <input
              value={form.version}
              onChange={(event) => setForm((state) => ({ ...state, version: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="latest"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Format</span>
            <input
              value={form.format}
              onChange={(event) => setForm((state) => ({ ...state, format: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="gguf"
            />
          </label>
          <label className="flex flex-col text-sm md:col-span-2">
            <span className="font-medium">Source URI</span>
            <input
              value={form.sourceUri}
              onChange={(event) => setForm((state) => ({ ...state, sourceUri: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="https://huggingface.co/..."
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Checksum (SHA-256)</span>
            <input
              value={form.checksum ?? ""}
              onChange={(event) => setForm((state) => ({ ...state, checksum: event.target.value || undefined }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="optional"
            />
          </label>
          <label className="flex flex-col text-sm">
            <span className="font-medium">Size (bytes)</span>
            <input
              type="number"
              value={form.sizeBytes ?? ""}
              onChange={(event) =>
                setForm((state) => ({ ...state, sizeBytes: event.target.value ? Number(event.target.value) : undefined }))
              }
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
            />
          </label>
          <label className="flex flex-col text-sm md:col-span-2">
            <span className="font-medium">Tags</span>
            <input
              value={form.tags}
              onChange={(event) => setForm((state) => ({ ...state, tags: event.target.value }))}
              className="mt-1 rounded-md border border-slate-300 px-3 py-2 shadow-sm"
              placeholder="general,quantized"
            />
          </label>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={handleRegister}
            className="rounded-md bg-slate-900 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-slate-700"
          >
            Register model
          </button>
        </div>
      </div>

      {error ? <p className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p> : null}

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
          <header className="flex items-center justify-between">
            <div>
              <h3 className="text-lg font-semibold">Registered models</h3>
              <p className="text-xs uppercase tracking-wide text-slate-500">
                {loading ? "Refreshing list…" : `${models.length} item${models.length === 1 ? "" : "s"}`}
              </p>
            </div>
            <button
              type="button"
              onClick={() => void loadModels()}
              className="rounded-md border border-slate-200 bg-white px-3 py-2 text-xs font-semibold uppercase tracking-wide"
            >
              Refresh
            </button>
          </header>
          <div className="mt-4 space-y-3">
            {models.length === 0 ? (
              <p className="text-sm text-slate-500">No models registered yet.</p>
            ) : (
              models.map((model) => (
                <article
                  key={model.id}
                  className={`rounded-md border px-4 py-3 text-sm shadow-sm transition hover:border-slate-400 ${
                    selectedModel?.id === model.id
                      ? "border-slate-900 bg-slate-900/5"
                      : "border-slate-200 bg-white"
                  }`}
                >
                  <button
                    type="button"
                    className="w-full text-left"
                    onClick={() => setSelectedModel(model)}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold">{model.name}</span>
                      <span className="rounded-full bg-slate-100 px-2 py-1 text-xs font-medium text-slate-600">
                        {model.stage}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-slate-500">
                      {model.provider} · v{model.version} · {model.format}
                    </p>
                    <p className="mt-1 text-xs text-slate-500">
                      Last synced: {model.last_synced_at ? new Date(model.last_synced_at).toLocaleString() : "never"}
                    </p>
                    {model.error_message ? (
                      <p className="mt-2 text-xs text-red-600">{model.error_message}</p>
                    ) : null}
                  </button>
                  <div className="mt-3 flex gap-2">
                    <button
                      type="button"
                      className="rounded-md border border-slate-200 px-3 py-1 text-xs font-medium"
                      onClick={() => void handleRefresh(model)}
                    >
                      Resync
                    </button>
                    <button
                      type="button"
                      className="rounded-md border border-red-200 bg-red-50 px-3 py-1 text-xs font-medium text-red-700"
                      onClick={() => void handleDelete(model)}
                    >
                      Delete
                    </button>
                  </div>
                </article>
              ))
            )}
          </div>
        </section>

        <section className="rounded-lg border border-slate-200 bg-white p-6 shadow-sm">
          <header className="space-y-1">
            <h3 className="text-lg font-semibold">Download & validation jobs</h3>
            <p className="text-xs uppercase tracking-wide text-slate-500">
              {selectedModel ? selectedModel.name : "Select a model to inspect jobs"}
            </p>
          </header>
          {jobsLoading ? (
            <p className="mt-4 text-sm text-slate-500">Loading jobs…</p>
          ) : jobs.length === 0 ? (
            <p className="mt-4 text-sm text-slate-500">
              {selectedModel ? "No download jobs recorded yet." : "Choose a model to display job history."}
            </p>
          ) : (
            <div className="mt-4 space-y-3">
              {jobs.map((job) => {
                const percent = Number.isFinite(job.progress)
                  ? Math.max(0, Math.min(100, Math.round(job.progress * 100)))
                  : 0;
                return (
                  <article
                    key={job.id}
                    className="rounded-md border border-slate-200 bg-slate-50 px-4 py-3 text-sm shadow-sm"
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold">{job.stage}</span>
                      <span className="text-xs text-slate-500">{percent}%</span>
                    </div>
                  <p className="text-xs text-slate-500">
                    Started {new Date(job.started_at).toLocaleString()}
                    {job.finished_at ? ` · Finished ${new Date(job.finished_at).toLocaleString()}` : ""}
                  </p>
                  {job.error_message ? <p className="mt-1 text-xs text-red-600">{job.error_message}</p> : null}
                  </article>
                );
              })}
            </div>
          )}
        </section>
      </div>
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
