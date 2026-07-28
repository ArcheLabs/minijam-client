import { expect, test } from "@playwright/test";
import { initialState, mockPlaygroundApi } from "./mock-api";

test("builds the Counter C example and displays its hash", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (request) => requests.push(request.url()));
  await mockPlaygroundApi(page, initialState());
  await page.goto("/");
  await page.getByRole("button", { name: "Build service" }).click();
  await expect(page.getByText("SUCCEEDED")).toBeVisible();
  await expect(page.getByText(`0x${"11".repeat(32)}`)).toBeVisible();
  expect(requests.join("\n")).not.toMatch(/ws:\/\/|wss:\/\/|:9944|internal\/v1\/compile|worker/i);
});
