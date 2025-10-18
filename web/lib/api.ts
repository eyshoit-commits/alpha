export const DEFAULT_DAEMON_URL = process.env.NEXT_PUBLIC_DAEMON_URL ?? "http://localhost:8080";

export interface ApiClientOptions {
  baseUrl?: string;
  token?: string;
  fetchImpl?: typeof fetch;
}

export interface ApiErrorBody {
  error: string;
  [key: string]: unknown;
}

export class DaemonApiError extends Error {
  public readonly status: number;
  public readonly body?: ApiErrorBody | unknown;

  constructor(message: string, status: number, body?: ApiErrorBody | unknown) {
    super(message);
    this.name = "DaemonApiError";
    this.status = status;
    this.body = body;
  }
}

export interface SandboxLimits {
  cpu_millis: number;
  memory_mib: number;
  disk_mib: number;
  timeout_seconds: number;
}

export interface Sandbox {
  id: string;
  namespace: string;
  name: string;
  runtime: string;
  status: string;
  limits: SandboxLimits;
  created_at: string;
  updated_at: string;
  last_started_at?: string | null;
  last_stopped_at?: string | null;
}

export interface ExecutionRecord {
  command: string;
  args: string[];
  executed_at: string;
  exit_code: number | null;
  stdout?: string | null;
  stderr?: string | null;
  duration_ms: number;
  timed_out: boolean;
}

export type KeyScope =
  | { type: "admin" }
  | { type: "namespace"; namespace: string };

export interface KeyInfo {
  id: string;
  scope: KeyScope;
  rate_limit: number;
  created_at: string;
  last_used_at?: string | null;
  expires_at?: string | null;
  key_prefix: string;
  rotated_from?: string | null;
  rotated_at?: string | null;
}

export interface IssuedKeyResponse {
  token: string;
  info: KeyInfo;
}

export interface RotationWebhookPayload {
  event: string;
  key_id: string;
  previous_key_id: string;
  rotated_at: string;
  scope: KeyScope;
  owner: string;
  key_prefix: string;
}

export interface RotationWebhookResponse {
  event_id: string;
  signature: string;
  payload: RotationWebhookPayload;
}

export interface RotatedKeyResponse {
  token: string;
  info: KeyInfo;
  previous: KeyInfo;
  webhook: RotationWebhookResponse;
}

export interface CreateSandboxPayload {
  namespace: string;
  name: string;
  runtime?: string;
  limits?: Partial<SandboxLimits>;
}

export interface ExecPayload {
  command: string;
  args?: string[];
  stdin?: string;
  timeout_ms?: number;
}

export interface CreateKeyPayload {
  scope: KeyScope;
  rate_limit?: number;
  ttl_seconds?: number;
}

export interface RotateKeyPayload {
  key_id: string;
  rate_limit?: number;
  ttl_seconds?: number;
}

export class ApiClient {
  private readonly baseUrl: string;
  private token?: string;
  private readonly fetchImpl: typeof fetch;

  constructor(options: ApiClientOptions = {}) {
    this.baseUrl = options.baseUrl ?? DEFAULT_DAEMON_URL;
    this.token = options.token;
    this.fetchImpl = options.fetchImpl ?? fetch;
  }

  withToken(token: string): ApiClient {
    const next = new ApiClient({
      baseUrl: this.baseUrl,
      fetchImpl: this.fetchImpl,
    });
    next.token = token;
    return next;
  }

  setToken(token?: string) {
    this.token = token;
  }

  async listSandboxes(namespace: string): Promise<Sandbox[]> {
    const params = new URLSearchParams({ namespace });
    return this.request<Sandbox[]>(`/api/v1/sandboxes?${params.toString()}`);
  }

  async createSandbox(payload: CreateSandboxPayload): Promise<Sandbox> {
    return this.request<Sandbox>(`/api/v1/sandboxes`, {
      method: "POST",
      body: payload,
    });
  }

  async startSandbox(id: string): Promise<Sandbox> {
    return this.request<Sandbox>(`/api/v1/sandboxes/${id}/start`, { method: "POST" });
  }

  async stopSandbox(id: string): Promise<void> {
    await this.request<void>(`/api/v1/sandboxes/${id}/stop`, { method: "POST" });
  }

  async deleteSandbox(id: string): Promise<void> {
    await this.request<void>(`/api/v1/sandboxes/${id}`, { method: "DELETE" });
  }

  async getSandbox(id: string): Promise<Sandbox> {
    return this.request<Sandbox>(`/api/v1/sandboxes/${id}/status`);
  }

  async exec(id: string, payload: ExecPayload): Promise<ExecutionRecord> {
    return this.request<ExecutionRecord>(`/api/v1/sandboxes/${id}/exec`, {
      method: "POST",
      body: payload,
    });
  }

  async listExecutions(id: string, limit = 20): Promise<ExecutionRecord[]> {
    const params = new URLSearchParams({ limit: limit.toString() });
    return this.request<ExecutionRecord[]>(`/api/v1/sandboxes/${id}/executions?${params.toString()}`);
  }

  async listKeys(): Promise<KeyInfo[]> {
    return this.request<KeyInfo[]>(`/api/v1/auth/keys`);
  }

  async issueKey(payload: CreateKeyPayload): Promise<IssuedKeyResponse> {
    return this.request<IssuedKeyResponse>(`/api/v1/auth/keys`, {
      method: "POST",
      body: payload,
    });
  }

  async rotateKey(payload: RotateKeyPayload): Promise<RotatedKeyResponse> {
    return this.request<RotatedKeyResponse>(`/api/v1/auth/keys/rotate`, {
      method: "POST",
      body: payload,
    });
  }

  async verifyRotationWebhook(payload: RotationWebhookPayload, signature: string): Promise<void> {
    await this.request<void>(`/api/v1/auth/keys/rotated`, {
      method: "POST",
      body: payload,
      headers: {
        "X-Cave-Webhook-Signature": signature,
      },
    });
  }

  async revokeKey(id: string): Promise<void> {
    await this.request<void>(`/api/v1/auth/keys/${id}`, { method: "DELETE" });
  }

  private async request<T>(
    path: string,
    options: { method?: string; body?: unknown; headers?: Record<string, string> } = {},
  ): Promise<T> {
    const url = new URL(path, this.baseUrl);
    const headers: Record<string, string> = {
      Accept: "application/json",
    };
    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }

    if (options.headers) {
      Object.assign(headers, options.headers);
    }

    let body: BodyInit | undefined;
    if (options.body !== undefined) {
      headers["Content-Type"] = "application/json";
      body = JSON.stringify(options.body);
    }

    const response = await this.fetchImpl(url.toString(), {
      method: options.method ?? "GET",
      headers,
      body,
    });

    if (response.status === 204) {
      return undefined as T;
    }

    const contentType = response.headers.get("content-type") ?? "";
    const isJson = contentType.includes("application/json");
    const payload = isJson ? await response.json() : await response.text();

    if (!response.ok) {
      const message = isJson && payload && typeof payload.error === "string"
        ? payload.error
        : response.statusText || "Request failed";
      throw new DaemonApiError(message, response.status, payload);
    }

    return payload as T;
  }
}

export const sharedApiClient = new ApiClient();
