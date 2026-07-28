import { blake2AsHex } from "@polkadot/util-crypto";

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, child]) => [key, canonicalize(child)])
    );
  }
  return value;
}

export function canonicalParamsJson(params: Record<string, unknown>): string {
  return JSON.stringify(canonicalize(params));
}

export function paramsHash(params: Record<string, unknown>): string {
  return blake2AsHex(new TextEncoder().encode(canonicalParamsJson(params)), 256);
}
