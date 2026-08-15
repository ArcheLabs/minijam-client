import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { u8aToHex, u8aWrapBytes } from "@polkadot/util";
import { cryptoWaitReady, sr25519PairFromSeed, sr25519Sign } from "@polkadot/util-crypto";
import { paramsHash } from "../src/actions/hash";

test.describe.configure({ mode: "serial" });

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const composeFile = process.env.MINIJAM_E2E_COMPOSE_FILE
  ?? path.join(repository, "deploy/season2/compose.compact.yml");
const composeProject = process.env.MINIJAM_E2E_COMPOSE_PROJECT ?? "minijam-season2-e2e";

function compose(...args: string[]) {
  const composeArgs = ["compose", "--project-name", composeProject, "-f", composeFile, ...args];
  execFileSync("docker", composeArgs, { encoding: "utf8", timeout: 120_000 });
}

async function waitFor<T>(read: () => Promise<T>, ready: (value: T) => boolean, timeout = 180_000) {
  const deadline = Date.now() + timeout;
  let last: T | undefined;
  while (Date.now() < deadline) {
    last = await read();
    if (ready(last)) return last;
    await new Promise((resolve) => setTimeout(resolve, 2_000));
  }
  throw new Error(`timed out waiting for state: ${JSON.stringify(last)}`);
}

async function signAction(page: Page,
                          pair: ReturnType<typeof sr25519PairFromSeed>,
                          account: string, action: string, params: Record<string, unknown>) {
  const preparedResponse = await page.request.post("/api/v1/actions/prepare", {
    data: {
      account,
      action,
      paramsHash: paramsHash(params),
      expiry: Math.floor(Date.now() / 1000) + 180
    }
  });
  expect(preparedResponse.ok()).toBeTruthy();
  const prepared = await preparedResponse.json() as { actionId: string; signingPayload: string };
  const payload = Uint8Array.from(
    prepared.signingPayload.slice(2).match(/.{2}/g)!.map((byte: string) => Number.parseInt(byte, 16))
  );
  return {
    actionId: prepared.actionId,
    signature: u8aToHex(sr25519Sign(u8aWrapBytes(payload), pair))
  };
}

test("fresh Season 2 chain executes Work and applies one replay-safe Allocation", async ({ page }) => {
  const seedHex = process.env.MINIJAM_E2E_WALLET_SEED;
  if (!seedHex) throw new Error("MINIJAM_E2E_WALLET_SEED must be set");
  const seed = Uint8Array.from(
    seedHex.replace(/^0x/, "").match(/.{2}/g)?.map((byte) => Number.parseInt(byte, 16)) ?? []
  );
  if (seed.length !== 32) throw new Error("MINIJAM_E2E_WALLET_SEED must be 32-byte hex");
  await cryptoWaitReady();
  const controller = sr25519PairFromSeed(seed);
  const controllerAddress = u8aToHex(controller.publicKey);
  await page.exposeFunction("__minijamE2eSign", (payloadHex: string) => {
    const payload = Uint8Array.from(
      payloadHex.slice(2).match(/.{2}/g)!.map((byte) => Number.parseInt(byte, 16))
    );
    return u8aToHex(sr25519Sign(u8aWrapBytes(payload), controller));
  });
  await page.addInitScript(({ injectedAddress }) => {
    const target = window as typeof window & {
      injectedWeb3?: Record<string, unknown>;
      __minijamE2eSign?: (payloadHex: string) => Promise<string>;
    };
    target.injectedWeb3 = {
      minijamE2e: {
        version: "1",
        enable: async () => ({
          accounts: { get: async () => [{ address: injectedAddress, name: "Season 2 E2E", type: "sr25519" }] },
          signer: { signRaw: async ({ data }: { data: string }) => ({ id: 1, signature: await target.__minijamE2eSign!(data) }) }
        })
      }
    };
  }, { injectedAddress: controllerAddress });

  await page.goto("/");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  await page.getByRole("button", { name: "Build service" }).click();
  await expect(page.getByText("SUCCEEDED")).toBeVisible();
  await page.getByRole("button", { name: "Deploy service" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page.getByText("Completed")).toBeVisible();
  const serviceButton = page.getByRole("button", { name: /Open Service/ });
  const serviceId = Number((await serviceButton.textContent())?.match(/\d+/)?.[0]);
  expect(serviceId).toBeGreaterThan(0);
  await serviceButton.click();
  await expect(page.getByText("Available")).toBeVisible();

  const serviceBeforeWork = await page.request.get(`/api/v1/services/${serviceId}`);
  expect(serviceBeforeWork.ok()).toBeTruthy();
  const service = await serviceBeforeWork.json() as { codeHash: string; balance: number; preimageReady: boolean };
  expect(service.preimageReady).toBeTruthy();
  const workParams = {
    serviceId,
    serviceCodeHash: service.codeHash,
    payloadBase64: "AQAAAAAAAAA=",
    extrinsicsBase64: []
  };
  async function submitWork(pair: ReturnType<typeof sr25519PairFromSeed>, account: string) {
    const authorization = await signAction(page, pair, account, "work", workParams);
    const response = await page.request.post("/api/v1/work", {
      data: { authorization, ...workParams }
    });
    expect(response.ok()).toBeTruthy();
    const operation = await response.json() as { operationId: string };
    await waitFor(async () => {
      const status = await page.request.get(`/api/v1/operations/${operation.operationId}`);
      return await status.json() as { status: string };
    }, (value) => value.status === "succeeded");
  }

  await submitWork(controller, controllerAddress);
  const nonController = sr25519PairFromSeed(new Uint8Array(32).fill(8));
  await submitWork(nonController, u8aToHex(nonController.publicKey));

  const allocationId = 7001;
  const amount = 500;
  const allocation = { allocationId, targetService: serviceId, amount };
  const initialBalance = service.balance;
  const submitAllocation = () => page.request.post("/api/v1/allocations", { data: allocation });
  expect((await submitAllocation()).ok()).toBeTruthy();
  await waitFor(async () => {
    const response = await page.request.get(`/api/v1/services/${serviceId}`);
    return await response.json() as { balance: number };
  }, (value) => value.balance === initialBalance + amount);

  expect((await submitAllocation()).ok()).toBeFalsy();
  const afterReplay = await (await page.request.get(`/api/v1/services/${serviceId}`)).json() as { balance: number };
  expect(afterReplay.balance).toBe(initialBalance + amount);

  compose("restart", "node", "worker", "api");
  await waitFor(async () => (await page.request.get("/health/ready")).status(), (status) => status === 204);
  expect((await submitAllocation()).ok()).toBeFalsy();
  const afterRestartReplay = await (await page.request.get(`/api/v1/services/${serviceId}`)).json() as { balance: number };
  expect(afterRestartReplay.balance).toBe(initialBalance + amount);

  const allocationTwo = { allocationId: allocationId + 1, targetService: serviceId, amount };
  expect((await page.request.post("/api/v1/allocations", { data: allocationTwo })).ok()).toBeTruthy();
  await waitFor(async () => {
    const response = await page.request.get(`/api/v1/services/${serviceId}`);
    return await response.json() as { balance: number };
  }, (value) => value.balance === initialBalance + amount * 2);
});
