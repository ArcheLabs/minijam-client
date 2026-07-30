export interface BuildRequest {
  language: "c" | "cpp";
  source: string;
  optimization: string;
}

export interface BuildArtifact {
  success: boolean;
  blobBase64?: string;
  codeHash?: string;
  codeLength?: number;
  toolchain: { clang: string; polkavm: string; converter: string; sdk: string };
  diagnostics: string[];
}

export interface PreparedAction {
  actionId: string;
  account: string;
  action: string;
  paramsHash: string;
  domain: string;
  genesis: string;
  expiry: number;
  signingPayload: string;
}

export interface Authorization {
  actionId: string;
  signature: string;
}

export interface Operation {
  operationId: string;
  kind: "create" | "upgrade" | "work";
  status: "prepared" | "submitted" | "waiting_receipt" | "submitting_preimage" | "waiting_preimage" | "tracking_work" | "succeeded" | "failed";
  request: Record<string, unknown>;
  correlation?: string;
  extrinsicHash?: string;
  submittedNonce?: number;
  result?: { serviceId?: number; workId?: number; executionReceipt?: string; preimageHash?: string };
  error?: string;
}

export interface ServiceView {
  serviceId: number;
  controller: string;
  codeHash: string;
  codeLength: number;
  preimageReady: boolean;
  finalizedBlock: string;
  finalizedBlockNumber: number;
}

export type FaucetStatus = "requesting" | "broadcasting" | "broadcasted" | "included" | "succeeded" | "limited" | "over-cap" | "unavailable" | "failed";

export interface FaucetEvent {
  status: FaucetStatus;
  hash?: string;
  blockHash?: string;
  error?: string;
}

export class PlaygroundApiError extends Error {
  constructor(
    public readonly status: number,
    message: string
  ) {
    super(message);
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, {
      ...init,
      headers: { "content-type": "application/json", ...init?.headers }
    });
  } catch {
    throw new PlaygroundApiError(0, "The Playground API is unavailable. Check your network and try again.");
  }
  if (!response.ok) {
    const body = await response.json().catch(() => ({})) as { error?: string };
    throw new PlaygroundApiError(response.status, body.error ?? `Request failed with status ${response.status}.`);
  }
  return response.json() as Promise<T>;
}

export const playgroundApi = {
  getConfig: () =>
    request<{ genesisHash: string; actionDomain: string }>("/api/v1/config"),
  build: (body: BuildRequest) =>
    request<BuildArtifact>("/api/v1/build", { method: "POST", body: JSON.stringify(body) }),
  prepareAction: (body: { account: string; action: string; paramsHash: string; expiry: number }) =>
    request<PreparedAction>("/api/v1/actions/prepare", { method: "POST", body: JSON.stringify(body) }),
  createService: (body: Record<string, unknown>) =>
    request<Operation>("/api/v1/services", { method: "POST", body: JSON.stringify(body) }),
  upgradeService: (serviceId: number, body: Record<string, unknown>) =>
    request<Operation>(`/api/v1/services/${serviceId}/upgrade`, { method: "POST", body: JSON.stringify(body) }),
  submitWork: (body: Record<string, unknown>) =>
    request<Operation>("/api/v1/work", { method: "POST", body: JSON.stringify(body) }),
  getOperation: (operationId: string) =>
    request<Operation>(`/api/v1/operations/${encodeURIComponent(operationId)}`),
  getService: (serviceId: number) =>
    request<ServiceView>(`/api/v1/services/${serviceId}`),
  getServiceStorage: (serviceId: number, key: string) =>
    request<{ serviceId: number; key: string; value?: string; finalizedBlock: string }>(
      `/api/v1/services/${serviceId}/storage?key=${encodeURIComponent(key)}`
    ),
  getFaucetBalance: (address: string) =>
    request<{ transferable: string; reserved: string; overCap: boolean }>(
      `/faucet/balance/${encodeURIComponent(address)}`
    )
};

export async function requestFaucet(
  body: { address: string; signature: string; message: string },
  onEvent: (event: FaucetEvent) => void
): Promise<void> {
  let response: Response;
  try {
    response = await fetch("/faucet/drip/web", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body)
    });
  } catch {
    onEvent({ status: "unavailable", error: "The faucet API is unavailable." });
    return;
  }
  if (!response.ok || !response.body) {
    onEvent({ status: "unavailable", error: `Faucet request failed with status ${response.status}.` });
    return;
  }

  const decoder = new TextDecoder();
  const reader = response.body.getReader();
  let buffer = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      const event = JSON.parse(line) as { status?: string; hash?: string; blockHash?: string; error?: string };
      if (event.error) {
        const error = event.error;
        const lowered = error.toLowerCase();
        onEvent({
          status: lowered.includes("quota") ? "limited" : lowered.includes("balance cap") ? "over-cap" : "failed",
          error
        });
      } else if (event.status) {
        onEvent({ status: event.status === "included" ? "succeeded" : event.status as FaucetStatus, hash: event.hash, blockHash: event.blockHash });
      }
    }
  }
}
