import type { Page } from "@playwright/test";
import { u8aToHex } from "@polkadot/util";
import { cryptoWaitReady, sr25519PairFromSeed } from "@polkadot/util-crypto";

export interface MockState {
  codeHash: string;
  counter: bigint;
  controller?: string;
  operationPolls: Map<string, number>;
  replayAction?: string;
  storageMissing?: boolean;
}

export const initialState = (): MockState => ({
  codeHash: `0x${"11".repeat(32)}`,
  counter: 0n,
  operationPolls: new Map()
});

export async function mockPlaygroundApi(page: Page, state: MockState) {
  await page.route("**/api/v1/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const body = request.postDataJSON?.() as Record<string, unknown> | undefined;
    const json = (value: unknown, status = 200) => route.fulfill({
      status,
      contentType: "application/json",
      body: JSON.stringify(value)
    });

    if (url.pathname === "/api/v1/build") {
      const nextHash = String(body?.source).includes("UPGRADED")
        ? `0x${"22".repeat(32)}`
        : state.codeHash;
      return json({
        success: true,
        blobBase64: "AQIDBA==",
        codeHash: nextHash,
        codeLength: 4,
        diagnostics: [],
        toolchain: { clang: "clang-19", polkavm: "0.24", converter: "v1", sdk: "stage0" }
      });
    }
    if (url.pathname === "/api/v1/config") {
      return json({
        genesisHash: `0x${"00".repeat(32)}`,
        actionDomain: "minijam/playground-action/v1"
      });
    }
    if (url.pathname === "/api/v1/actions/prepare") {
      return json({
        actionId: `0x${"33".repeat(32)}`,
        account: body?.account,
        action: body?.action,
        paramsHash: body?.paramsHash,
        domain: "minijam/playground-action/v1",
        genesis: `0x${"00".repeat(32)}`,
        expiry: body?.expiry,
        signingPayload: `0x${"44".repeat(32)}`
      });
    }
    if (url.pathname === "/api/v1/services" && request.method() === "POST") {
      if (state.replayAction) return json({ error: "signed action was already used" }, 409);
      state.controller = await testAccountId();
      state.replayAction = String((body?.authorization as { actionId: string }).actionId);
      state.operationPolls.set("create-op", 0);
      return json(operation("create-op", "create", "waiting_receipt"));
    }
    if (/\/api\/v1\/services\/7\/upgrade$/.test(url.pathname)) {
      state.codeHash = String(body?.codeHash);
      state.operationPolls.set("upgrade-op", 0);
      return json(operation("upgrade-op", "upgrade", "waiting_receipt"));
    }
    if (url.pathname === "/api/v1/work") {
      state.counter += 1n;
      state.operationPolls.set("work-op", 0);
      return json({
        ...operation("work-op", "work", "tracking_work"),
        request: { serviceId: 7, packageHash: `0x${"55".repeat(32)}`, bundleCid: "bafk2bzacebundle" }
      });
    }
    const operationMatch = url.pathname.match(/^\/api\/v1\/operations\/(.+)$/);
    if (operationMatch) {
      const id = operationMatch[1];
      const polls = (state.operationPolls.get(id) ?? 0) + 1;
      state.operationPolls.set(id, polls);
      const kind = id.startsWith("create") ? "create" : id.startsWith("upgrade") ? "upgrade" : "work";
      const done = polls > 1;
      return json({
        ...operation(id, kind, done ? "succeeded" : kind === "work" ? "tracking_work" : "waiting_receipt"),
        account: await testAccountId(),
        request: kind === "work" ? { serviceId: 7, packageHash: `0x${"55".repeat(32)}`, bundleCid: "bafk2bzacebundle" } : {},
        result: done ? (kind === "create" ? { serviceId: 7 } : kind === "work" ? { workId: 9, executionReceipt: `0x${"66".repeat(32)}` } : { serviceId: 7 }) : undefined
      });
    }
    if (url.pathname === "/api/v1/services/7/storage") {
      if (state.storageMissing) {
        return json({
          serviceId: 7,
          key: url.searchParams.get("key"),
          value: null,
          finalizedBlock: `0x${"77".repeat(32)}`
        });
      }
      const bytes = new Uint8Array(8);
      new DataView(bytes.buffer).setBigInt64(0, state.counter, true);
      return json({ serviceId: 7, key: url.searchParams.get("key"), value: hex(bytes), finalizedBlock: `0x${"77".repeat(32)}` });
    }
    if (url.pathname === "/api/v1/services/7") {
      return json({
        serviceId: 7,
        controller: state.controller ?? await testAccountId(),
        codeHash: state.codeHash,
        codeLength: 4,
        preimageReady: true,
        finalizedBlock: `0x${"77".repeat(32)}`,
        finalizedBlockNumber: 12
      });
    }
    return json({ error: `unmocked ${url.pathname}` }, 404);
  });
}

function operation(operationId: string, kind: string, status: string) {
  return { operationId, kind, status, account: "", actionId: "", request: {}, createdAt: 1, updatedAt: 1 };
}

async function testAccountId() {
  await cryptoWaitReady();
  return u8aToHex(sr25519PairFromSeed(new Uint8Array(32).fill(7)).publicKey);
}

function hex(bytes: Uint8Array) {
  return `0x${[...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}
