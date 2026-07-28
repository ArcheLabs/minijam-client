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
  status: "prepared" | "submitted" | "waiting_receipt" | "submitting_preimage" | "tracking_work" | "succeeded" | "failed";
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
    )
};
