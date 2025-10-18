import { test, expect } from "@playwright/test";
import { randomUUID } from "crypto";

test("namespace operator can execute commands", async ({ page }) => {
  const sandboxes = [
    {
      id: randomUUID(),
      namespace: "default",
      name: "ci-runner",
      runtime: "python",
      status: "stopped",
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
    },
  ];

  let lastExecution = {
    command: "python",
    args: ["--version"],
    executed_at: new Date().toISOString(),
    exit_code: 0,
    stdout: "Python 3.12.1",
    stderr: null,
    duration_ms: 42,
    timed_out: false,
  };

  await page.route("**/api/v1/sandboxes", async (route) => {
    const request = route.request();
    if (request.method() === "GET") {
      return route.fulfill({ status: 200, body: JSON.stringify(sandboxes) });
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

  await page.route(/\/api\/v1\/sandboxes\/.*\/stop/, async (route) => {
    const id = route.request().url().split("/").slice(-2, -1)[0];
    const sandbox = sandboxes.find((item) => item.id === id);
    if (sandbox) {
      sandbox.status = "stopped";
      sandbox.updated_at = new Date().toISOString();
      return route.fulfill({ status: 204, body: "" });
    }
    return route.fulfill({ status: 404, body: JSON.stringify({ error: "not found" }) });
  });

  await page.route(/\/api\/v1\/sandboxes\/.*\/executions.*/, async (route) => {
    return route.fulfill({ status: 200, body: JSON.stringify([lastExecution]) });
  });

  await page.route(/\/api\/v1\/sandboxes\/.*\/exec/, async (route) => {
    const body = route.request().postDataJSON() as { command: string; args?: string[] };
    lastExecution = {
      command: body.command,
      args: body.args ?? [],
      executed_at: new Date().toISOString(),
      exit_code: 0,
      stdout: "ok",
      stderr: null,
      duration_ms: 15,
      timed_out: false,
    };
    return route.fulfill({ status: 200, body: JSON.stringify(lastExecution) });
  });

  await page.goto("/");

  await page.getByLabel("Namespace token").fill("ns-token");
  await page.getByRole("button", { name: "Save token" }).click();

  await page.getByRole("button", { name: "ci-runner" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByText("running", { exact: false })).toBeVisible();

  await page.getByLabel("Command").fill("/bin/echo");
  await page.getByLabel("Arguments").fill("phase-0");
  await page.getByRole("button", { name: "Run command" }).click();

  await expect(page.getByText("stdout")).toBeVisible();
  await expect(page.getByText("phase-0")).toBeVisible();
});
