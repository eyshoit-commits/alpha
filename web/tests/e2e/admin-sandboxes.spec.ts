import { test, expect } from "@playwright/test";
import { randomUUID } from "crypto";

test("admin can create and review sandboxes", async ({ page }) => {
  const sandboxes: any[] = [];
  const executions: Record<string, any[]> = {};

  await page.route("**/api/v1/sandboxes", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (request.method() === "GET") {
      if (!url.searchParams.has("namespace")) {
        return route.fulfill({ status: 400, body: JSON.stringify({ error: "namespace required" }) });
      }
      return route.fulfill({ status: 200, body: JSON.stringify(sandboxes) });
    }

    if (request.method() === "POST") {
      const body = request.postDataJSON() as { name: string; namespace: string };
      const sandbox = {
        id: randomUUID(),
        namespace: body.namespace,
        name: body.name,
        runtime: "nodejs",
        status: "pending",
        limits: {
          cpu_millis: 2000,
          memory_mib: 1024,
          disk_mib: 1024,
          timeout_seconds: 120,
        },
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        last_started_at: null,
        last_stopped_at: null,
      };
      sandboxes.push(sandbox);
      executions[sandbox.id] = [
        {
          command: "echo",
          args: ["hello"],
          executed_at: new Date().toISOString(),
          exit_code: 0,
          stdout: "hello",
          stderr: null,
          duration_ms: 10,
          timed_out: false,
        },
      ];
      return route.fulfill({ status: 200, body: JSON.stringify(sandbox) });
    }

    return route.continue();
  });

  await page.route(/\/api\/v1\/sandboxes\/.*\/start/, async (route) => {
    const id = route.request().url().split("/").slice(-2, -1)[0];
    const sandbox = sandboxes.find((item) => item.id === id);
    if (sandbox) {
      sandbox.status = "running";
      sandbox.updated_at = new Date().toISOString();
      return route.fulfill({ status: 200, body: JSON.stringify(sandbox) });
    }
    return route.fulfill({ status: 404, body: JSON.stringify({ error: "not found" }) });
  });

  await page.route(/\/api\/v1\/sandboxes\/.*\/executions.*/, async (route) => {
    const id = route.request().url().split("/").slice(-2, -1)[0];
    return route.fulfill({
      status: 200,
      body: JSON.stringify(executions[id] ?? []),
    });
  });

  await page.goto("/auth/token?returnTo=/sandboxes");
  await page.getByLabel("Daemon API token").fill("admin-token");
  await page.getByRole("button", { name: "Save token" }).click();
  await page.waitForURL(/\/sandboxes$/);

  await page.getByLabel("Sandbox name").fill("ci-runner");
  await page.getByRole("button", { name: "Create sandbox" }).click();

  await expect(page.getByText("ci-runner")).toBeVisible();

  await page.getByRole("button", { name: "ci-runner" }).click();
  await expect(page.getByText("hello")).toBeVisible();

  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("running", { exact: false })).toBeVisible();
});
