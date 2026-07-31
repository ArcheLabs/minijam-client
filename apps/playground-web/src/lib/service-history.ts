const STORAGE_KEY = "minijam.playground.services.v1";

export interface RememberedService {
  serviceId: number;
  codeHash?: string;
}

type ServiceHistory = Record<string, Record<string, RememberedService[]>>;

export function rememberService(
  genesisHash: string,
  accountId: string,
  service: RememberedService
) {
  const history = readHistory();
  const genesis = normalize(genesisHash);
  const account = normalize(accountId);
  const services = history[genesis]?.[account] ?? [];
  const existing = services.find((item) => item.serviceId === service.serviceId);
  const next = existing
    ? services.map((item) => item.serviceId === service.serviceId ? { ...item, ...service } : item)
    : [...services, service];

  history[genesis] ??= {};
  history[genesis][account] = next;
  writeHistory(history);
}

export function readRememberedServices(genesisHash: string, accountId: string) {
  const history = readHistory();
  return history[normalize(genesisHash)]?.[normalize(accountId)] ?? [];
}

export function removeRememberedService(
  genesisHash: string,
  accountId: string,
  serviceId: number
) {
  const history = readHistory();
  const genesis = normalize(genesisHash);
  const account = normalize(accountId);
  const services = history[genesis]?.[account];
  if (!services) return;

  history[genesis][account] = services.filter((item) => item.serviceId !== serviceId);
  writeHistory(history);
}

function readHistory(): ServiceHistory {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (!value) return {};
    const parsed = JSON.parse(value) as unknown;
    return isHistory(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function writeHistory(history: ServiceHistory) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
}

function normalize(value: string) {
  return value.toLowerCase();
}

function isHistory(value: unknown): value is ServiceHistory {
  if (!isRecord(value)) return false;
  return Object.values(value).every((accounts) =>
    isRecord(accounts) && Object.values(accounts).every((services) =>
      Array.isArray(services) && services.every((service) =>
        isRecord(service) &&
        typeof service.serviceId === "number" &&
        Number.isSafeInteger(service.serviceId) &&
        service.serviceId >= 0 &&
        (service.codeHash === undefined || typeof service.codeHash === "string")
      )
    )
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
