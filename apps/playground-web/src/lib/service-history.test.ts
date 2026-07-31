import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  readRememberedServices,
  rememberService,
  removeRememberedService
} from "./service-history";

const values = new Map<string, string>();

beforeEach(() => {
  values.clear();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value)
  });
});

describe("service history", () => {
  it("isolates by genesis and normalized account without duplicates", () => {
    rememberService("0xAA", "0xABCD", { serviceId: 7, codeHash: "0x11" });
    rememberService("0xaa", "0xabcd", { serviceId: 7, codeHash: "0x22" });
    rememberService("0xBB", "0xabcd", { serviceId: 8 });

    expect(readRememberedServices("0xAa", "0xAbCd")).toEqual([
      { serviceId: 7, codeHash: "0x22" }
    ]);
    expect(readRememberedServices("0xbb", "0xABCD")).toEqual([{ serviceId: 8 }]);
  });

  it("removes one service and tolerates corrupted storage", () => {
    rememberService("0xaa", "0xabcd", { serviceId: 7 });
    rememberService("0xaa", "0xabcd", { serviceId: 8 });
    removeRememberedService("0xAA", "0xABCD", 7);
    expect(readRememberedServices("0xaa", "0xabcd")).toEqual([{ serviceId: 8 }]);

    values.set("minijam.playground.services.v1", "{broken");
    expect(readRememberedServices("0xaa", "0xabcd")).toEqual([]);
  });
});
