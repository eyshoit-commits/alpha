import { expect, test } from "@playwright/test";
import { randomUUID } from "crypto";

test("admin can rotate and acknowledge API keys", async ({ page }) => {
  const keys = [
    {
      id: randomUUID(),
      scope: { type: "namespace", namespace: "demo" },
      rate_limit: 100,
      created_at: new Date().toISOString(),
      last_used_at: null,
      expires_at: null,
      key_prefix: "bkg_demo_1234",
      rotated_from: null,
      rotated_at: null,
    },
  ];

  const rotationWebhook = {
    event: "key.rotated",
    key_id: keys[0].id,
    previous_key_id: keys[0].id,
    rotated_at: new Date().toISOString(),
    scope: keys[0].scope,
    owner: "demo",
    key_prefix: "bkg_demo_5678",
  };

  await page.route("**/api/v1/auth/keys", async (route) => {
    const request = route.request();
    if (request.method() === "GET") {
      return route.fulfill({ status: 200, body: JSON.stringify(keys) });
    }
    return route.continue();
  });

  await page.route("**/api/v1/auth/keys/rotate", async (route) => {
    const rotated = {
      id: randomUUID(),
      scope: keys[0].scope,
      rate_limit: 80,
      created_at: new Date().toISOString(),
      last_used_at: null,
      expires_at: null,
      key_prefix: "bkg_demo_5678",
      rotated_from: keys[0].id,
      rotated_at: new Date().toISOString(),
    };
    keys.push(rotated as any);
    return route.fulfill({
      status: 200,
      body: JSON.stringify({
        token: "bkg_demo_rotated_token",
        info: rotated,
        previous: keys[0],
        webhook: {
          event_id: randomUUID(),
          signature: "signed",
          payload: rotationWebhook,
        },
      }),
    });
  });

  await page.route("**/api/v1/auth/keys/rotated", async (route) => {
    const signature = route.request().headers()["x-cave-webhook-signature"];
    expect(signature).toBe("signed");
    return route.fulfill({ status: 204, body: "" });
  });

  await page.goto("/auth/token?returnTo=/keys");
  await page.getByLabel("Daemon API token").fill("admin-token");
  await page.getByRole("button", { name: "Save token" }).click();
  await page.waitForURL(/\/keys$/);

  await page.getByRole("button", { name: "Refresh list" }).click();
  await expect(page.getByText("bkg_demo_1234")).toBeVisible();

  await page.getByLabel("Key to rotate").selectOption(keys[0].id);
  await page.getByLabel("New rate limit (req/min)").fill("80");
  await page.getByRole("button", { name: "Rotate key" }).click();

  await expect(page.getByText("Rotation completed")).toBeVisible();
  await expect(page.getByText("bkg_demo_rotated_token")).toBeVisible();

  await page.getByRole("button", { name: "Acknowledge webhook" }).click();
  await expect(page.getByText("Rotation webhook acknowledged")).toBeVisible();
});
