import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

test.describe.configure({ mode: "serial" });

test("Build, signed Create, Work, finalized state, and signed Upgrade cross processes", async ({ page }) => {
  const browserRequests: string[] = [];
  page.on("request", (request) => browserRequests.push(request.url()));

  await page.goto("/");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  await page.getByRole("button", { name: "Build service" }).click();
  await expect(page.getByText("SUCCEEDED")).toBeVisible();
  const initialCodeHash = await page.locator(".artifact .mono").first().textContent();

  await page.getByRole("button", { name: "Deploy service" }).click();
  await expect(page.getByRole("dialog")).toContainText("Create Service");
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page).toHaveURL(/\/operations\//);
  await expect(page.getByText("Completed")).toBeVisible();
  const openService = page.getByRole("button", { name: /Open Service/ });
  const serviceId = Number((await openService.textContent())?.match(/\d+/)?.[0]);
  expect(serviceId).toBeGreaterThan(0);
  await openService.click();

  await expect(page.getByText("Available")).toBeVisible();
  await page.getByRole("button", { name: "Run Work" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page.getByText("Completed")).toBeVisible();
  await expect(page.getByText("Work ID").locator("..")).toBeVisible();
  await page.getByRole("button", { name: "View finalized Service state" }).click();
  await page.getByRole("button", { name: "Read finalized value" }).click();
  await expect(page.getByText("Counter: 1")).toBeVisible();
  const workerLogs = composeOutput("logs", "--no-color", "worker-1", "worker-2", "worker-3");
  expect(workerLogs).toMatch(/submitted_candidates=[1-9]/);
  expect(workerLogs).toMatch(/vote_tasks_or_submitted=[1-9]/);
  for (const worker of ["worker-1", "worker-2", "worker-3"]) {
    const metrics = composeOutput("exec", "-T", worker, "curl", "-fsS", "http://127.0.0.1:9616/metrics");
    expect(metrics).toMatch(/minijam_worker_bundle_ready_total [1-9]/);
  }

  const editor = page.locator(".monaco-editor");
  await editor.click();
  await page.keyboard.press("Control+End");
  await page.keyboard.type("\n/* stage0-upgrade */");
  await page.getByRole("button", { name: "Build upgrade" }).click();
  const upgradedCodeHash = page.locator(".upgrade-actions .mono");
  await expect(upgradedCodeHash).not.toHaveText("No upgraded artifact yet");
  const upgradedHash = await upgradedCodeHash.textContent();
  expect(upgradedHash).not.toBe(initialCodeHash);
  await page.getByRole("button", { name: "Upgrade Service" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page.getByText("Completed")).toBeVisible();
  await page.getByRole("button", { name: `Open Service ${serviceId}` }).click();
  await expect(page.getByText(upgradedHash ?? "")).toBeVisible();
  await expect(page.getByText("Available")).toBeVisible();

  const external = browserRequests.filter((url) =>
    /ws:\/\/|wss:\/\/|:9944|internal\/v1\/compile|worker-[123]/i.test(url)
  );
  expect(external).toEqual([]);
});

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const composeFile = path.join(repository, "deploy/local/docker-compose.yml");
const envFile = process.env.MINIJAM_STAGE0_ENV ?? path.join(repository, "deploy/local/.env");

function composeOutput(...args: string[]) {
  const composeArgs = [
    "compose",
    "--project-name", "minijam-stage0",
  ];
  if (process.env.MINIJAM_STAGE0_ENV) composeArgs.push("--env-file", envFile);
  composeArgs.push("-f", composeFile, ...args);
  return execFileSync("docker", composeArgs, { encoding: "utf8", timeout: 120_000 });
}
