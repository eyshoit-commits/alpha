import { expect, test } from "@playwright/test";
import { randomUUID } from "crypto";

test("admin can filter audit events", async ({ page }) => {
  const events = [
    {
      id: randomUUID(),
      namespace: "demo",
      actor: "system",
      event_type: "sandbox_exec",
      recorded_at: new Date().toISOString(),
      payload: { command: "echo", exit_code: 0 },
      signature_valid: true,
    },
    {
      id: randomUUID(),
      namespace: "ops",
      actor: "ops-user",
      event_type: "sandbox_start",
      recorded_at: new Date().toISOString(),
      payload: { sandbox: "ops-runner" },
      signature_valid: false,
    },
  ];

  await page.route("**/api/v1/audit/events", async (route) => {
    const url = new URL(route.request().url());
    const namespace = url.searchParams.get("namespace");
    const eventType = url.searchParams.get("event_type");
    const filtered = events.filter((event) => {
      if (namespace && event.namespace !== namespace) {
        return false;
      }
      if (eventType && event.event_type !== eventType) {
        return false;
      }
      return true;
    });
    return route.fulfill({ status: 200, body: JSON.stringify(filtered) });
  });

  await page.goto("/auth/token?returnTo=/audit");
  await page.getByLabel("Daemon API token").fill("admin-token");
  await page.getByRole("button", { name: "Save token" }).click();
  await page.waitForURL(/\/audit$/);

  await expect(page.getByText("sandbox_exec")).toBeVisible();
  await expect(page.getByText("sandbox_start")).toBeVisible();

  await page.getByLabel("Namespace").fill("ops");
  await page.getByRole("button", { name: "Apply filters" }).click();

  await expect(page.getByText("sandbox_start")).toBeVisible();
  await expect(page.getByText("sandbox_exec")).toHaveCount(0);

  await page.getByRole("button", { name: "Reset" }).click();
  await expect(page.getByText("sandbox_exec")).toBeVisible();
});
