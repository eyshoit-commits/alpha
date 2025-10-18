import { test, expect } from "@playwright/test";

import {
  ApiClient,
  RotateKeyPayload,
  RotationWebhookPayload,
  RotatedKeyResponse,
} from "../lib/api";

test("rotateKey posts payload and returns response", async () => {
  const payload: RotateKeyPayload = {
    key_id: "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
    rate_limit: 250,
    ttl_seconds: 3600,
  };

  const responseBody: RotatedKeyResponse = {
    token: "new-token",
    info: {
      id: "5f86a0ef-55c0-4f50-a1e9-b85a2b3db0fe",
      scope: { type: "admin" },
      rate_limit: 250,
      created_at: new Date().toISOString(),
      last_used_at: null,
      expires_at: null,
      key_prefix: "new-token-prefix",
      rotated_from: payload.key_id,
      rotated_at: new Date().toISOString(),
    },
    previous: {
      id: payload.key_id,
      scope: { type: "admin" },
      rate_limit: 200,
      created_at: new Date().toISOString(),
      last_used_at: new Date().toISOString(),
      expires_at: null,
      key_prefix: "old-prefix",
      rotated_from: null,
      rotated_at: new Date().toISOString(),
    },
    webhook: {
      event_id: "6b4dc7a8-1e5a-4cfa-a2e2-f9d4f2b1c90c",
      signature: "signature",
      payload: {
        event: "key.rotated",
        key_id: "5f86a0ef-55c0-4f50-a1e9-b85a2b3db0fe",
        previous_key_id: payload.key_id,
        rotated_at: new Date().toISOString(),
        scope: { type: "admin" },
        owner: "admin",
        key_prefix: "new-token-prefix",
      },
    },
  };

  const requests: Record<string, RequestInit> = {};

  const client = new ApiClient({
    baseUrl: "https://daemon.example",
    token: "admin-token",
    fetchImpl: async (url, init) => {
      requests[url.toString()] = init ?? {};
      const parsed = JSON.parse(init?.body as string);
      expect(parsed).toEqual(payload);
      expect(init?.headers).toMatchObject({
        Accept: "application/json",
        "Content-Type": "application/json",
        Authorization: "Bearer admin-token",
      });
      expect(init?.method).toBe("POST");
      return new Response(JSON.stringify(responseBody), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    },
  });

  const result = await client.rotateKey(payload);
  expect(result.token).toBe(responseBody.token);
  expect(Object.keys(requests)).toEqual(["https://daemon.example/api/v1/auth/keys/rotate"]);
});

test("verifyRotationWebhook sends signature header", async () => {
  const payload: RotationWebhookPayload = {
    event: "key.rotated",
    key_id: "5f86a0ef-55c0-4f50-a1e9-b85a2b3db0fe",
    previous_key_id: "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
    rotated_at: new Date().toISOString(),
    scope: { type: "admin" },
    owner: "admin",
    key_prefix: "prefix",
  };

  let lastRequest: RequestInit | undefined;

  const client = new ApiClient({
    baseUrl: "https://daemon.example",
    token: "admin-token",
    fetchImpl: async (_url, init) => {
      lastRequest = init ?? {};
      return new Response(null, { status: 204 });
    },
  });

  await client.verifyRotationWebhook(payload, "signed-value");

  expect(lastRequest?.method).toBe("POST");
  expect(lastRequest?.headers).toMatchObject({
    Accept: "application/json",
    Authorization: "Bearer admin-token",
    "Content-Type": "application/json",
    "X-Cave-Webhook-Signature": "signed-value",
  });
  expect(JSON.parse(lastRequest?.body as string)).toEqual(payload);
});
