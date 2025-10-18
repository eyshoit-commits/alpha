import { expect, test } from "@playwright/test";
import { randomUUID } from "crypto";

test("admin can register and manage models", async ({ page }) => {
  const models: any[] = [];
  const jobs: Record<string, any[]> = {};

  await page.route("**/api/v1/models", async (route) => {
    const request = route.request();
    if (request.method() === "GET") {
      return route.fulfill({ status: 200, body: JSON.stringify(models) });
    }
    if (request.method() === "POST") {
      const body = request.postDataJSON() as { name: string; version: string; format: string; source_uri: string };
      const model = {
        id: randomUUID(),
        name: body.name,
        provider: "huggingface",
        version: body.version,
        format: body.format,
        source_uri: body.source_uri,
        size_bytes: null,
        checksum_sha256: null,
        stage: "queued",
        last_synced_at: null,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        tags: [],
        error_message: null,
      };
      models.push(model);
      jobs[model.id] = [
        {
          id: randomUUID(),
          model_id: model.id,
          stage: "downloading",
          progress: 0.5,
          started_at: new Date().toISOString(),
          finished_at: null,
          error_message: null,
        },
      ];
      return route.fulfill({ status: 200, body: JSON.stringify(model) });
    }
    return route.continue();
  });

  await page.route(/\/api\/v1\/models\/.*\/refresh/, async (route) => {
    const id = route.request().url().split("/").at(-2)!;
    const model = models.find((entry) => entry.id === id);
    if (model) {
      model.stage = "ready";
      model.last_synced_at = new Date().toISOString();
      model.updated_at = new Date().toISOString();
      jobs[id].push({
        id: randomUUID(),
        model_id: id,
        stage: "ready",
        progress: 1,
        started_at: new Date().toISOString(),
        finished_at: new Date().toISOString(),
        error_message: null,
      });
      return route.fulfill({ status: 200, body: JSON.stringify(model) });
    }
    return route.fulfill({ status: 404, body: JSON.stringify({ error: "not found" }) });
  });

  await page.route(/\/api\/v1\/models\/.*\/jobs/, async (route) => {
    const id = route.request().url().split("/").at(-2)!;
    return route.fulfill({ status: 200, body: JSON.stringify(jobs[id] ?? []) });
  });

  await page.goto("/auth/token?returnTo=/models");
  await page.getByLabel("Daemon API token").fill("admin-token");
  await page.getByRole("button", { name: "Save token" }).click();
  await page.waitForURL(/\/models$/);

  await page.getByLabel("Model name").fill("phi-3");
  await page.getByLabel("Source URI").fill("https://huggingface.co/phi-3");
  await page.getByRole("button", { name: "Register model" }).click();

  await expect(page.getByText("phi-3")).toBeVisible();
  await expect(page.getByText("queued")).toBeVisible();

  await page.getByRole("button", { name: "Resync" }).click();
  await expect(page.getByText("ready")).toBeVisible();

  await expect(page.getByText("Download & validation jobs")).toBeVisible();
  await expect(page.getByText("Finished", { exact: false })).toBeVisible();
});
