import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

describe("browser security boundary", () => {
  it("contains no Node, Worker, Compiler-internal, or secret endpoints", () => {
    const source = files(join(process.cwd(), "src"))
      .filter((path) => !path.endsWith(".test.ts"))
      .map((path) => readFileSync(path, "utf8"))
      .join("\n");
    expect(source).not.toMatch(/ws:\/\/|wss:\/\/|:9944|internal\/v1\/compile|author_submitExtrinsic|WORKER_SIGNING_KEY|RELAYER.*SEED/i);
  });
});

function files(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    return statSync(path).isDirectory() ? files(path) : /\.(ts|tsx|css|c|cpp)$/.test(path) ? [path] : [];
  });
}
