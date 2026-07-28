import { describe, expect, it } from "vitest";
import vectors from "../../../../test-vectors/playground-actions.json";
import { paramsHash } from "./hash";

describe("Playground action golden vectors", () => {
  for (const vector of vectors) {
    it(vector.name, () => {
      expect(paramsHash(vector.params)).toBe(vector.paramsHash);
    });
  }
});
