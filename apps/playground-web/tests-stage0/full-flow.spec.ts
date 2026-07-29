import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { u8aToHex } from "@polkadot/util";
import { cryptoWaitReady, sr25519PairFromSeed, sr25519Sign } from "@polkadot/util-crypto";
import { paramsHash } from "../src/actions/hash";

test.describe.configure({ mode: "serial" });

let createdServiceId = 0;

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
  createdServiceId = serviceId;
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
  const workerMetrics = ["worker-1", "worker-2", "worker-3"].map((worker) =>
    composeOutput("exec", "-T", worker, "curl", "-fsS", "http://127.0.0.1:9616/metrics")
  );
  expect(workerMetrics.some((metrics) =>
    /minijam_worker_bundle_ready_total [1-9]/.test(metrics)
  )).toBeTruthy();

  const editor = page.locator(".monaco-editor");
  await editor.click();
  const upgradedSource = readFileSync(
    path.join(repository, "apps/playground-web/src/examples/counter.c"),
    "utf8"
  ).replace("minijam_refine_error(1)", "minijam_refine_error(2)");
  await page.keyboard.press("Control+A");
  await page.keyboard.insertText(upgradedSource);
  await page.getByRole("button", { name: "Build upgrade" }).click();
  const upgradedCodeHash = page.locator(".upgrade-actions .mono");
  await expect(upgradedCodeHash).not.toHaveText("No upgraded artifact yet");
  const upgradedHash = await upgradedCodeHash.textContent();
  expect(upgradedHash).toMatch(/^0x[0-9a-f]{64}$/i);
  expect(upgradedHash).not.toBe(initialCodeHash);
  await page.getByRole("button", { name: "Upgrade Service" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page.getByText("Completed")).toBeVisible();
  await page.getByRole("button", { name: `Open Service ${serviceId}` }).click();
  await expect(page.getByText(upgradedHash ?? "")).toBeVisible();
  await expect(page.getByText("Available")).toBeVisible();

  await cryptoWaitReady();
  const nonController = sr25519PairFromSeed(new Uint8Array(32).fill(8));
  const params = {
    serviceId,
    serviceCodeHash: upgradedHash!,
    payloadBase64: "AQAAAAAAAAA=",
    extrinsicsBase64: []
  };
  const expiry = Math.floor(Date.now() / 1000) + 120;
  const preparedResponse = await page.request.post("/api/v1/actions/prepare", {
    data: {
      account: u8aToHex(nonController.publicKey),
      action: "work",
      paramsHash: paramsHash(params),
      expiry
    }
  });
  expect(preparedResponse.ok()).toBeTruthy();
  const prepared = await preparedResponse.json() as { actionId: string; signingPayload: string };
  const payload = Uint8Array.from(prepared.signingPayload.slice(2).match(/.{2}/g)!.map((byte) => Number.parseInt(byte, 16)));
  const signature = u8aToHex(sr25519Sign(payload, nonController));
  const nonceBefore = await relayerNonce();
  const forbidden = await page.request.post("/api/v1/work", {
    data: { authorization: { actionId: prepared.actionId, signature }, ...params }
  });
  expect(forbidden.status()).toBe(403);
  expect(await relayerNonce()).toBe(nonceBefore);

  const external = browserRequests.filter((url) =>
    /ws:\/\/|wss:\/\/|:9944|internal\/v1\/compile|worker-[123]/i.test(url)
  );
  expect(external).toEqual([]);
});

test("a non-terminal Work survives Playground and Worker restart without a second submission", async ({ page }) => {
  expect(createdServiceId).toBeGreaterThan(0);
  await page.goto(`/services/${createdServiceId}`);
  await page.getByRole("button", { name: "Connect wallet" }).click();

  compose("stop", "worker-1", "worker-2", "worker-3");
  await page.getByRole("button", { name: "Run Work" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page).toHaveURL(/\/operations\//);
  const operationUrl = page.url();
  await expect(page.getByText(/Processing work|Submitted|Preparing/)).toBeVisible();

  compose("restart", "playground-api");
  compose("restart", "worker-2");
  compose("start", "worker-1", "worker-3");
  await page.reload();
  expect(page.url()).toBe(operationUrl);
  await expect(page.getByText("Completed")).toBeVisible();
  await expect(page.getByText("Work ID").locator("..")).toHaveCount(1);
  await page.getByRole("button", { name: "View finalized Service state" }).click();
  await page.getByRole("button", { name: "Read finalized value" }).click();
  await expect(page.getByText("Counter: 2")).toBeVisible();

  const state = composeOutput("exec", "-T", "worker-2", "cat", "/data/state.toml");
  const workKeys = [...state.matchAll(/work_id\s*=\s*(\d+)/g)].map((match) => match[1]);
  expect(new Set(workKeys).size).toBe(workKeys.length);
  const recoveryLogs = composeOutput("logs", "--no-color", "playground-api", "worker-1", "worker-2", "worker-3");
  expect(recoveryLogs).not.toMatch(/duplicate (work|candidate|vote)|already (submitted|voted)/i);
});

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const composeFile = path.join(repository, "deploy/local/docker-compose.yml");
const envFile = process.env.MINIJAM_STAGE0_ENV ?? path.join(repository, "deploy/local/.env");

function compose(...args: string[]) {
  composeOutput(...args);
}

function composeOutput(...args: string[]) {
  const composeArgs = [
    "compose",
    "--project-name", "minijam-stage0",
  ];
  if (process.env.MINIJAM_STAGE0_ENV) composeArgs.push("--env-file", envFile);
  composeArgs.push("-f", composeFile, ...args);
  return execFileSync("docker", composeArgs, { encoding: "utf8", timeout: 120_000 });
}

async function relayerNonce() {
  const response = await fetch(`http://127.0.0.1:${process.env.MINIJAM_NODE_PORT ?? "9944"}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      id: 1,
      jsonrpc: "2.0",
      method: "system_accountNextIndex",
      params: ["0x901578a417300aa0ae533b5bd0e9af489a4cc4a6f38999b76283867087738209"]
    })
  });
  const body = await response.json() as { result: number };
  return body.result;
}
